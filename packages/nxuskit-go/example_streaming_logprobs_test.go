package nxuskit_test

import (
	"context"
	"fmt"

	nxuskit "github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go"
)

// Example_streamingLogprobs demonstrates iterating per-chunk streaming
// logprob deltas using the mock provider with a deterministic configuration.
// Non-supporting providers leave chunk.Logprobs == nil (FR-007).
func Example_streamingLogprobs() {
	chunks := []nxuskit.StreamChunk{{Delta: "Hello"}}
	deltas := [][]*nxuskit.StreamLogprobsDelta{
		{
			{Content: []nxuskit.TokenLogprob{
				{Token: "Hello", Logprob: -0.05},
			}},
		},
	}

	provider := nxuskit.NewMockProvider(
		nxuskit.WithMockStreamResponse(chunks),
		nxuskit.WithStreamingLogprobs(deltas),
	)

	chunkCh, _ := provider.ChatStream(context.Background(), &nxuskit.ChatRequest{
		Model:    "mock-model",
		Messages: []nxuskit.Message{nxuskit.UserMessage("hi")},
	})

	for chunk := range chunkCh {
		if chunk.Logprobs != nil {
			for _, tok := range chunk.Logprobs.Content {
				fmt.Printf("token=%q logprob=%.2f\n", tok.Token, tok.Logprob)
			}
		}
	}
	// Output: token="Hello" logprob=-0.05
}
