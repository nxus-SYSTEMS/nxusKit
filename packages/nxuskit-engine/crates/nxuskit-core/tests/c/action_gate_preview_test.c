/*
 * Executable C consumer for the package-owned Action Gate Python Preview.
 * Fixture JSON arrives through the Rust harness, so this program performs no
 * file, network, provider, customer-data, or action-execution operation.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "nxuskit.h"

#define CHECK(condition, message)                                              \
    do {                                                                       \
        if (!(condition)) {                                                    \
            const char *error = nxuskit_last_error();                          \
            fprintf(stderr, "FAIL: %s (last error: %s)\n", (message),          \
                    error != NULL ? error : "(none)");                         \
            return 1;                                                          \
        }                                                                      \
    } while (0)

static int contains(const char *json, const char *needle) {
    return json != NULL && strstr(json, needle) != NULL;
}

static int evaluate_fixture(const char *request, const char *decision) {
    char decision_fragment[64];
    char *result;

    CHECK(request != NULL && request[0] != '\0',
          "fixture request must be supplied");
    snprintf(decision_fragment, sizeof(decision_fragment),
             "\"decision\":\"%s\"", decision);
    result = nxuskit_action_gate_preview_evaluate_json(request);
    CHECK(result != NULL, "Action Gate preview C ABI returned NULL");
    CHECK(contains(result, decision_fragment), "fixture decision must match");
    CHECK(!contains(result, "synthetic.actor"),
          "result must not echo actor references");
    CHECK(!contains(result, "token"), "result must not echo token material");
    CHECK(contains(result, "\"action_execution_authorized\":false"),
          "preview must not authorize action execution");
    nxuskit_free_string(result);
    return 0;
}

int main(void) {
    const char *allow = getenv("NXUSKIT_ACTION_GATE_ALLOW_FIXTURE");
    const char *deny = getenv("NXUSKIT_ACTION_GATE_DENY_FIXTURE");
    const char *review = getenv("NXUSKIT_ACTION_GATE_REVIEW_FIXTURE");
    const char *review_missing_handoff =
        getenv("NXUSKIT_ACTION_GATE_REVIEW_MISSING_HANDOFF_FIXTURE");
    const char *missing_evidence =
        getenv("NXUSKIT_ACTION_GATE_MISSING_EVIDENCE_FIXTURE");
    const char *forbidden_authority =
        getenv("NXUSKIT_ACTION_GATE_FORBIDDEN_AUTHORITY_FIXTURE");
    char *malformed;

    if (evaluate_fixture(allow, "allow") != 0 ||
        evaluate_fixture(deny, "deny") != 0 ||
        evaluate_fixture(review, "review_required") != 0 ||
        evaluate_fixture(review_missing_handoff, "deny") != 0 ||
        evaluate_fixture(missing_evidence, "deny") != 0 ||
        evaluate_fixture(forbidden_authority, "deny") != 0) {
        return 1;
    }

    malformed = nxuskit_action_gate_preview_evaluate_json("{");
    CHECK(malformed != NULL,
          "malformed preview request must return structured deny");
    CHECK(!contains(malformed, "\"decision\":\"allow\""),
          "malformed preview request must never return allow");
    CHECK(contains(malformed, "\"status\":\"invalid_request\""),
          "malformed preview request must report invalid_request");
    nxuskit_free_string(malformed);

    puts("PASS: C ABI Action Gate preview consumer");
    return 0;
}
