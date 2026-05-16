import json
from pathlib import Path


def test_v092_plain_dict_chat_request_serializes_byte_identically_without_logprobs():
    request = {
        "model": "gpt-5.4",
        "messages": [{"role": "user", "content": "Hello from v0.9.2"}],
        "stream": False,
    }

    actual = json.dumps(request, separators=(",", ":"))
    fixture = (
        Path(__file__).resolve().parents[2]
        / "nxuskit"
        / "tests"
        / "fixtures"
        / "v092-chat-request-no-logprobs.json"
    )
    expected = fixture.read_text().strip()

    assert actual == expected
    assert "logprobs" not in actual
    assert "top_logprobs" not in actual
