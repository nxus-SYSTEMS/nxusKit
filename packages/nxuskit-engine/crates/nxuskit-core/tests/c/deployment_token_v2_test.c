/*
 * Executable C-language consumer for the frozen deterministic
 * DeploymentClaimsV2 fixture. Tokens and the debug-only verifier PEM are
 * supplied by the Rust fixture harness; this C program exercises only the
 * published C ABI and asserts its safe JSON boundary.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "nxuskit.h"

#define CURRENT_KID                                                            \
    "projects/nxus-licensing-prod/locations/us-central1/keyRings/"             \
    "licensing-signing/cryptoKeys/license-token-signing/cryptoKeyVersions/1"

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

static int validate_v2_acceptance(const char *token) {
    char *result = nxuskit_license_validate(token);
    CHECK(result != NULL, "v2 C ABI validation returned NULL");

    CHECK(contains(result, "\"valid\":true"), "v2 fixture must validate");
    CHECK(contains(result, "\"token_type\":\"deployment\""),
          "C ABI must expose the deployment kind");
    CHECK(contains(result, "\"edition\":\"pro\""),
          "C ABI must expose the safe edition");
    CHECK(contains(result, "\"sdk_release_line\":2"),
          "C ABI must expose the v2 release line");
    CHECK(contains(result, "\"grant_revision\":3"),
          "C ABI must expose the safe grant revision");
    CHECK(contains(result, "\"authorization_scope\":\"client_entitlement\""),
          "C ABI must expose the safe authorization scope");
    CHECK(contains(result, "\"signing_key_id\":\"" CURRENT_KID "\""),
          "C ABI must expose the exact opaque signing kid");
    CHECK(contains(result, "\"token_fingerprint\":\"sha256:"),
          "C ABI must expose a token fingerprint instead of the token");
    CHECK(!contains(result, token), "C ABI must not echo the compact token");
    CHECK(!contains(result, "account_7c1d9a"),
          "C ABI must not echo the subject");
    CHECK(!contains(result, "grant_2f8b30"),
          "C ABI must not echo grant identifiers");

    nxuskit_free_string(result);
    return 0;
}

static int validate_v3_rejection(const char *token) {
    char *result = nxuskit_license_validate(token);
    CHECK(result != NULL, "v3 C ABI validation returned NULL");

    CHECK(contains(result, "\"valid\":false"),
          "a v3 release-line token must fail in the v2 C ABI runtime");
    CHECK(contains(result, "\"result\":\"incompatible_release_line\""),
          "v3 rejection must identify the safe release-line reason");
    CHECK(!contains(result, token),
          "rejected C ABI output must not echo the compact token");
    CHECK(!contains(result, "account_7c1d9a"),
          "rejected C ABI output must not echo the subject");

    nxuskit_free_string(result);
    return 0;
}

int main(void) {
    const char *v2_token = getenv("NXUSKIT_DEPLOYMENT_TOKEN_V2");
    const char *v3_token = getenv("NXUSKIT_DEPLOYMENT_TOKEN_V3");
    CHECK(
        v2_token != NULL && v2_token[0] != '\0',
        "NXUSKIT_DEPLOYMENT_TOKEN_V2 must supply the deterministic v2 fixture");
    CHECK(v3_token != NULL && v3_token[0] != '\0',
          "NXUSKIT_DEPLOYMENT_TOKEN_V3 must supply the deterministic v3 "
          "rejection fixture");

    if (validate_v2_acceptance(v2_token) != 0) {
        return 1;
    }
    if (validate_v3_rejection(v3_token) != 0) {
        return 1;
    }

    puts("PASS: C ABI DeploymentClaimsV2 fixture consumer");
    return 0;
}
