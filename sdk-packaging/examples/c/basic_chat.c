/*
 * nxuskit SDK Example — Basic Chat (C)
 *
 * Demonstrates: provider creation, synchronous chat, response reading, cleanup.
 *
 * Build:
 *   make basic_chat
 *
 * Run:
 *   export OPENAI_API_KEY="sk-..."
 *   ./bin/basic_chat
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "nxuskit.h"

int main(void) {
    /* Print library version */
    printf("nxuskit version: %s\n\n", nxuskit_version());

    /* Create an OpenAI provider.
     * The API key is read from the config JSON. You can also pass it
     * via environment variable on the Rust side, but explicit config
     * is more portable. */
    const char *api_key = getenv("OPENAI_API_KEY");
    if (!api_key) {
        fprintf(stderr, "Error: set OPENAI_API_KEY environment variable\n");
        return 1;
    }

    /* Build config JSON. In production, use a proper JSON library. */
    char config[512];
    snprintf(config, sizeof(config),
             "{\"provider_type\":\"openai\",\"api_key\":\"%s\"}", api_key);

    struct NxuskitProvider *provider = nxuskit_create_provider(config);
    if (!provider) {
        fprintf(stderr, "Failed to create provider: %s\n",
                nxuskit_last_error());
        return 1;
    }

    /* Send a chat request */
    const char *request = "{\"model\":\"gpt-4o-mini\","
                          "\"messages\":[{\"role\":\"user\","
                          "\"content\":\"What is the capital of France? Reply "
                          "in one sentence.\"}],"
                          "\"max_tokens\":100}";

    struct NxuskitResponse *response = nxuskit_chat(provider, request);
    if (!response) {
        fprintf(stderr, "Chat failed: %s\n", nxuskit_last_error());
        nxuskit_free_provider(provider);
        return 1;
    }

    /* Read and print the response */
    const char *json = nxuskit_response_json(response);
    printf("Response JSON:\n%s\n", json);

    /* Cleanup */
    nxuskit_free_response(response);
    nxuskit_free_provider(provider);

    return 0;
}
