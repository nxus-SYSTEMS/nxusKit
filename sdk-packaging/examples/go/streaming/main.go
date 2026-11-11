// nxuskit SDK Example - Streaming Chat (Go)
//
// Demonstrates streaming with channels using the nxuskit-go package.
//
// Run:
//   export OPENAI_API_KEY="sk-..."
//   go run ./streaming

package main

import (
	"context"
	"fmt"
	"log"
	"time"

	nxuskit "github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go"
)

func main() {
	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()

	// Start a streaming completion.
	// Returns two channels: chunks and errors.
	chunks, errs := nxuskit.CompletionStream(ctx, "gpt-4o-mini",
		"Count from 1 to 5, with a brief description for each number.",
		nxuskit.WithMaxTokens(200),
	)

	fmt.Print("Streaming: ")

	// Read chunks as they arrive.
	for chunk := range chunks {
		fmt.Print(chunk.Delta)
	}
	fmt.Println()

	// Check for errors.
	if err := <-errs; err != nil {
		log.Fatalf("Stream error: %v", err)
	}

	fmt.Println("\nDone.")
}
