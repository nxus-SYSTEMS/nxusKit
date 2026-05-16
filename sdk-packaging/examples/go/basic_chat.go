// nxuskit SDK Example — Basic Chat (Go)
//
// Uses the nxuskit-go package which delegates to libnxuskit via cgo.
//
// Run:
//   export OPENAI_API_KEY="sk-..."
//   go run basic_chat.go

package main

import (
	"context"
	"fmt"
	"log"
	"time"

	"github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go"
)

func main() {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	// Simple one-line completion with auto-detected provider.
	// The model prefix "gpt-" routes to the OpenAI provider automatically.
	resp, err := nxuskit-go.Completion(ctx, "gpt-4o-mini",
		"What is the capital of France? Reply in one sentence.",
		nxuskit-go.WithMaxTokens(100),
	)
	if err != nil {
		log.Fatalf("Completion failed: %v", err)
	}

	fmt.Printf("Response: %s\n", resp.Content)
	fmt.Printf("Model: %s\n", resp.Model)

	if resp.Usage.Actual != nil {
		fmt.Printf("Tokens: %d prompt + %d completion\n",
			resp.Usage.Actual.PromptTokens,
			resp.Usage.Actual.CompletionTokens,
		)
	}
}
