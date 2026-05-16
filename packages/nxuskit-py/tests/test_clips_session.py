"""Tests for the CLIPS Session wrapper.

Tests that require the native library are marked with the 'nxuskit' marker
and skipped when the library is not available.
"""

from __future__ import annotations

import pytest

# Try to import and load the native library — skip FFI tests if unavailable
try:
    from nxuskit._clips_ffi import _get_lib
    from nxuskit.clips import ClipsError, ClipsSession  # noqa: F401

    # Force library load — import alone doesn't trigger it (lazy loading)
    _get_lib()
    HAS_CLIPS = True
except Exception:
    HAS_CLIPS = False

needs_lib = pytest.mark.skipif(not HAS_CLIPS, reason="requires nxuskit native library")


@needs_lib
class TestClipsSessionLifecycle:
    def test_create_and_close(self):
        s = ClipsSession()
        s.close()

    def test_context_manager(self):
        with ClipsSession() as s:
            s.reset()

    def test_double_close(self):
        s = ClipsSession()
        s.close()
        s.close()  # should not raise


@needs_lib
class TestClipsSessionLoadJSON:
    def test_load_json_and_run_inference(self):
        with ClipsSession() as s:
            json_str = """{
                "templates": [
                    {
                        "name": "sensor",
                        "slots": [
                            {"name": "name", "type": "STRING"},
                            {"name": "value", "type": "INTEGER"}
                        ]
                    },
                    {
                        "name": "alert",
                        "slots": [
                            {"name": "sensor-name", "type": "STRING"},
                            {"name": "level", "type": "SYMBOL"}
                        ]
                    }
                ],
                "rules": [
                    {
                        "name": "check-high",
                        "source": "(defrule check-high (sensor (name ?n) (value ?v&:(> ?v 100))) => (assert (alert (sensor-name ?n) (level high))))"
                    }
                ]
            }"""
            s.load_json(json_str)
            s.reset()

            idx = s.fact_assert_string('(sensor (name "temp-1") (value 200))')
            assert idx >= 0

            fired = s.run()
            assert fired == 1

            alerts = s.facts_by_template("alert")
            assert len(alerts) == 1


@needs_lib
class TestClipsSessionTemplateOps:
    def test_template_exists_and_slots(self):
        with ClipsSession() as s:
            s.load_json("""{
                "templates": [
                    {
                        "name": "item",
                        "slots": [
                            {"name": "id", "type": "INTEGER"},
                            {"name": "name", "type": "STRING"}
                        ]
                    }
                ]
            }""")

            assert s.template_exists("item")
            names = s.template_slot_names("item")
            assert len(names) == 2


@needs_lib
class TestClipsSessionCache:
    def test_preload_get_cached_workflow(self):
        rules_json = """{
            "templates": [
                {
                    "name": "cached_item",
                    "slots": [{"name": "x", "type": "INTEGER"}]
                }
            ]
        }"""

        ClipsSession.preload("py-cache-test", rules_json)

        s1 = ClipsSession.get_cached("py-cache-test")
        s1.reset()
        assert s1.template_exists("cached_item")

        s2 = ClipsSession.get_cached("py-cache-test")
        s2.reset()

        # Modify s1
        s1.fact_assert_string("(cached_item (x 42))")

        # s2 should be unaffected
        facts = s2.facts_by_template("cached_item")
        assert len(facts) == 0

        s1.close()
        s2.close()
        ClipsSession.cache_remove("py-cache-test")


@needs_lib
class TestClipsSessionModules:
    def test_default_module_is_main(self):
        with ClipsSession() as s:
            mod = s.module_current_get()
            assert mod == "MAIN"
            assert s.module_exists("MAIN")


@needs_lib
class TestClipsSessionSettings:
    def test_fact_duplication_toggle(self):
        with ClipsSession() as s:
            s.fact_duplication_set(True)
            assert s.fact_duplication_get() is True

            s.fact_duplication_set(False)
            assert s.fact_duplication_get() is False


@needs_lib
class TestClipsSessionEval:
    def test_eval_arithmetic(self):
        with ClipsSession() as s:
            result = s.eval("(+ 2 3)")
            assert result is not None


@needs_lib
class TestClipsSessionFBPPattern:
    def test_multi_cycle_fbp(self):
        with ClipsSession() as s:
            s.load_json("""{
                "templates": [
                    {
                        "name": "data",
                        "slots": [
                            {"name": "key", "type": "STRING"},
                            {"name": "val", "type": "INTEGER"}
                        ]
                    },
                    {
                        "name": "result",
                        "slots": [
                            {"name": "key", "type": "STRING"},
                            {"name": "val", "type": "INTEGER"}
                        ]
                    }
                ],
                "rules": [
                    {
                        "name": "double-val",
                        "source": "(defrule double-val (data (key ?k) (val ?v)) => (assert (result (key ?k) (val (* ?v 2)))))"
                    }
                ]
            }""")

            for cycle in range(3):
                s.reset()
                for i in range(10):
                    s.fact_assert_string(f'(data (key "k{i}") (val {(cycle + 1) * 10 + i}))')

                fired = s.run()
                assert fired == 10, f"cycle {cycle}: expected 10 firings, got {fired}"

                results = s.facts_by_template("result")
                assert len(results) == 10, f"cycle {cycle}: expected 10 results, got {len(results)}"
