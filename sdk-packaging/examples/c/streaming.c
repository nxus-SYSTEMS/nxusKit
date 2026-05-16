/*
 * nxuskit SDK Example — Streaming Chat (C)
 *
 * Demonstrates: streaming chat with callbacks, chunk processing, cleanup.
 *
 * Build:
 *   make streaming
 *
 * Run:
 *   export OPENAI_API_KEY="sk-..."
 *   ./bin/streaming
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "nxuskit.h"

/* State passed through user_data to callbacks */
struct stream_state {
    int chunk_count;
    int done;
};

/* Called for each streaming chunk (from a background thread) */
static int32_t on_chunk(const char *chunk_json, void *user_data) {
    struct stream_state *state = (struct stream_state *)user_data;
    state->chunk_count++;

    /* In production, parse the JSON to extract content.
     * Here we print the raw JSON for demonstration. */
    printf("  chunk %d: %s\n", state->chunk_count, chunk_json);

    return 0; /* Return 0 to continue, non-zero to cancel */
}

/* Called once when streaming completes */
static void on_done(const char *final_json, void *user_data) {
    struct stream_state *state = (struct stream_state *)user_data;
    state->done = 1;

    printf("\nStream complete (%d chunks).\n", state->chunk_count);
    if (final_json) {
        printf("Final response: %.200s...\n", final_json);
    }
}

int main(void) {
    printf("nxuskit version: %s\n\n", nxuskit_version());

    const char *api_key = getenv("OPENAI_API_KEY");
    if (!api_key) {
        fprintf(stderr, "Error: set OPENAI_API_KEY environment variable\n");
        return 1;
    }

    char config[512];
    snprintf(config, sizeof(config),
             "{\"provider_type\":\"openai\",\"api_key\":\"%s\"}", api_key);

    struct NxuskitProvider *provider = nxuskit_create_provider(config);
    if (!provider) {
        fprintf(stderr, "Failed to create provider: %s\n",
                nxuskit_last_error());
        return 1;
    }

    /* Prepare streaming request */
    const char *request = "{\"model\":\"gpt-4o-mini\","
                          "\"messages\":[{\"role\":\"user\","
                          "\"content\":\"Count from 1 to 5, with a brief "
                          "description for each number.\"}],"
                          "\"max_tokens\":200,\"stream\":true}";

    struct stream_state state = {0, 0};

    printf("Starting stream...\n\n");

    struct NxuskitStream *stream =
        nxuskit_chat_stream(provider, request, on_chunk, on_done, &state);
    if (!stream) {
        fprintf(stderr, "Stream failed: %s\n", nxuskit_last_error());
        nxuskit_free_provider(provider);
        return 1;
    }

    /* Wait for streaming to complete.
     * In a real application, you might do other work here or use
     * a more sophisticated synchronization mechanism. */
    while (!state.done) {
        /* Busy-wait. In production, use a condition variable or similar. */
    }

    /* Cleanup */
    nxuskit_free_stream(stream);
    nxuskit_free_provider(provider);

    return 0;
}
