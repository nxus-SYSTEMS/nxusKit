/*
 * nxuskit C ABI Smoke Test
 *
 * Minimal C program that links against libnxuskit and verifies the core
 * lifecycle: version retrieval, provider creation (mock), synchronous chat,
 * response reading, and cleanup.
 *
 * Exit codes:
 *   0 — all checks passed
 *   1 — a check failed (details on stderr)
 */

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "nxuskit.h"

// clang-format off
#define CHECK(cond, msg)                                       \
    do {                                                       \
        if (!(cond)) {                                         \
            const char *err = nxuskit_last_error();            \
            fprintf(stderr, "FAIL: %s (error: %s)\n",         \
                    (msg), err ? err : "(none)");              \
            return 1;                                          \
        }                                                      \
    } while (0)
// clang-format on

int main(void) {
    int failures = 0;
    const char *status;
    const char *plural;

    /* ── Version ────────────────────────────────────────────── */
    printf("Test: nxuskit_version... ");
    const char *version = nxuskit_version();
    if (version == NULL || strlen(version) == 0) {
        fprintf(stderr, "FAIL: version is NULL or empty\n");
        failures++;
    } else {
        printf("OK (%s)\n", version);
    }

    /* ── Create mock provider ───────────────────────────────── */
    printf("Test: create mock provider... ");
    const char *config = "{\"provider_type\": \"mock\"}";
    struct NxuskitProvider *provider = nxuskit_create_provider(config);
    if (provider == NULL) {
        const char *err = nxuskit_last_error();
        fprintf(stderr, "FAIL: create_provider returned NULL (error: %s)\n",
                err ? err : "(none)");
        failures++;
        goto done;
    }
    printf("OK\n");

    /* ── Synchronous chat ───────────────────────────────────── */
    printf("Test: synchronous chat... ");
    // clang-format off
    const char *request =
        "{\"model\":\"test\",\"messages\":[{\"role\":\"user\","
        "\"content\":\"hello from C\"}]}";
    // clang-format on
    struct NxuskitResponse *response = nxuskit_chat(provider, request);
    if (response == NULL) {
        const char *err = nxuskit_last_error();
        fprintf(stderr, "FAIL: chat returned NULL (error: %s)\n",
                err ? err : "(none)");
        failures++;
        goto cleanup_provider;
    }
    printf("OK\n");

    /* ── Read response JSON ─────────────────────────────────── */
    printf("Test: response JSON... ");
    const char *json = nxuskit_response_json(response);
    if (json == NULL || strlen(json) == 0) {
        fprintf(stderr, "FAIL: response JSON is NULL or empty\n");
        failures++;
    } else if (strstr(json, "content") == NULL) {
        fprintf(stderr, "FAIL: response JSON missing 'content' field: %s\n",
                json);
        failures++;
    } else {
        printf("OK (%.60s...)\n", json);
    }

    /* ── Null safety ────────────────────────────────────────── */
    printf("Test: null safety... ");
    nxuskit_free_provider(NULL);
    nxuskit_free_response(NULL);
    nxuskit_free_string(NULL);
    struct NxuskitProvider *null_prov = nxuskit_create_provider(NULL);
    if (null_prov != NULL) {
        fprintf(stderr, "FAIL: create_provider(NULL) should return NULL\n");
        nxuskit_free_provider(null_prov);
        failures++;
    } else {
        printf("OK\n");
    }

    /* ── Cleanup ────────────────────────────────────────────── */
    nxuskit_free_response(response);
cleanup_provider:
    nxuskit_free_provider(provider);

    /* ── CLIPS Session API ──────────────────────────────────── */
    printf("\nTest: CLIPS session create... ");
    uint64_t session = nxuskit_clips_session_create();
    if (session == 0) {
        const char *err = nxuskit_last_error();
        fprintf(stderr, "FAIL: session_create returned 0 (error: %s)\n",
                err ? err : "(none)");
        failures++;
        goto done;
    }
    printf("OK (handle=%llu)\n", (unsigned long long)session);

    printf("Test: CLIPS load_json... ");
    {
        // clang-format off
        const char *rules_json =
            "{"
            "  \"templates\": ["
            "    {\"name\": \"sensor\", \"slots\": ["
            "      {\"name\": \"name\", \"type\": \"STRING\"},"
            "      {\"name\": \"value\", \"type\": \"INTEGER\"}"
            "    ]},"
            "    {\"name\": \"alert\", \"slots\": ["
            "      {\"name\": \"sensor-name\", \"type\": \"STRING\"},"
            "      {\"name\": \"level\", \"type\": \"SYMBOL\"}"
            "    ]}"
            "  ],"
            "  \"rules\": ["
            "    {\"name\": \"check-high\","
            "     \"source\": \"(defrule check-high "
            "(sensor (name ?n) (value ?v&:(> ?v 100)))"
            " => (assert (alert (sensor-name ?n) (level high))))\"}"
            "  ]"
            "}";
        // clang-format on
        int32_t rc = nxuskit_clips_session_load_json(session, rules_json);
        CHECK(rc == 0, "load_json failed");
        printf("OK\n");
    }

    printf("Test: CLIPS session reset... ");
    {
        int32_t rc = nxuskit_clips_session_reset(session);
        CHECK(rc == 0, "reset failed");
        printf("OK\n");
    }

    printf("Test: CLIPS fact_assert_string... ");
    {
        const char *fact1 = "(sensor (name \"temp-1\") (value 150))";
        int64_t idx = nxuskit_clips_fact_assert_string(session, fact1);
        CHECK(idx >= 0, "fact_assert_string failed");
        printf("OK (idx=%lld)\n", (long long)idx);
    }

    printf("Test: CLIPS run (should fire 1 rule)... ");
    {
        int64_t fired = nxuskit_clips_session_run(session, -1);
        if (fired != 1) {
            fprintf(stderr, "FAIL: expected 1 rule fired, got %lld\n",
                    (long long)fired);
            failures++;
        } else {
            printf("OK (1 rule)\n");
        }
    }

    printf("Test: CLIPS facts_by_template... ");
    {
        char *alerts_json = nxuskit_clips_facts_by_template(session, "alert");
        CHECK(alerts_json != NULL, "facts_by_template returned NULL");
        /* Should contain at least one fact index */
        CHECK(strlen(alerts_json) > 2, "expected non-empty alert list");
        printf("OK (%s)\n", alerts_json);
        nxuskit_free_string(alerts_json);
    }

    printf("Test: CLIPS template_exists... ");
    {
        bool exists = nxuskit_clips_template_exists(session, "sensor");
        CHECK(exists, "sensor template should exist");
        bool missing = nxuskit_clips_template_exists(session, "nonexistent");
        CHECK(!missing, "nonexistent template should not exist");
        printf("OK\n");
    }

    /* ── CLIPS multi-phase inference ─────────────────────── */
    printf("Test: CLIPS multi-phase inference... ");
    {
        /* Phase 2: reset, assert below-threshold value, verify no firing */
        int32_t rc = nxuskit_clips_session_reset(session);
        CHECK(rc == 0, "reset phase 2");

        const char *fact2 = "(sensor (name \"temp-2\") (value 50))";
        int64_t idx = nxuskit_clips_fact_assert_string(session, fact2);
        CHECK(idx >= 0, "assert_string phase 2");

        int64_t fired = nxuskit_clips_session_run(session, -1);
        if (fired != 0) {
            fprintf(stderr, "FAIL: phase 2 expected 0 rules, got %lld\n",
                    (long long)fired);
            failures++;
        } else {
            printf("OK (phase2=0)\n");
        }
    }

    /* ── CLIPS session cache ─────────────────────────────── */
    printf("Test: CLIPS session cache... ");
    {
        // clang-format off
        const char *cache_rules =
            "{\"templates\": [{\"name\": \"cached_t\","
            " \"slots\": [{\"name\": \"x\"}]}]}";
        // clang-format on
        int32_t rc =
            nxuskit_clips_session_preload("c-smoke-cache", cache_rules);
        CHECK(rc == 0, "preload failed");

        uint64_t cached = nxuskit_clips_session_get_cached("c-smoke-cache");
        CHECK(cached != 0, "get_cached returned 0");

        bool exists = nxuskit_clips_template_exists(cached, "cached_t");
        CHECK(exists, "cached_t template should exist in cached session");

        nxuskit_clips_session_destroy(cached);
        rc = nxuskit_clips_session_cache_remove("c-smoke-cache");
        CHECK(rc == 0, "cache_remove failed");
        printf("OK\n");
    }

    /* ── CLIPS null/stale handle safety ──────────────────── */
    printf("Test: CLIPS stale handle safety... ");
    nxuskit_clips_session_destroy(0); /* no-op for 0 handle */
    printf("OK\n");

    nxuskit_clips_session_destroy(session);
done:
    status = failures == 0 ? "ALL PASSED" : "FAILED";
    plural = failures == 1 ? "" : "s";
    printf("\n%s (%d failure%s)\n", status, failures, plural);
    return failures == 0 ? 0 : 1;
}
