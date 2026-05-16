package nxuskit

import (
	"time"
)

// WithSystemPrompt adds a system message to the beginning of the conversation.
// If used with Completion(), creates: [SystemMessage(prompt), UserMessage(input)]
// If used with CompletionWithMessages() and messages already has a system message,
// this prepends an additional system message.
//
// Example:
//
//	resp, err := nxuskit.Completion(ctx, "gpt-4o", "Hello",
//	    nxuskit.WithSystemPrompt("You are a helpful assistant."),
//	)
func WithSystemPrompt(prompt string) Option {
	return func(r *ChatRequest) error {
		if prompt == "" {
			return nil
		}

		// Prepend system message to existing messages
		r.Messages = append([]Message{SystemMessage(prompt)}, r.Messages...)
		return nil
	}
}

// WithTimeout sets a timeout for the request.
// If the context already has a deadline, the shorter of the two is used.
//
// Note: This option is applied by the convenience functions (Completion, etc.)
// and affects the context used for the provider call. It does not modify
// the ChatRequest directly.
//
// Default timeout is 2 minutes if neither context nor option specifies one.
//
// Example:
//
//	resp, err := nxuskit.Completion(ctx, "gpt-4o", "Complex question",
//	    nxuskit.WithTimeout(5*time.Minute),
//	)
func WithTimeout(d time.Duration) Option {
	return func(r *ChatRequest) error {
		// Store timeout in a way that convenience functions can access
		// We use a metadata field for this since ChatRequest doesn't have a timeout field
		if r.Metadata == nil {
			r.Metadata = make(map[string]any)
		}
		r.Metadata["_convenience_timeout"] = d
		return nil
	}
}

// WithImages adds images to the user message for vision-capable models.
// The images are attached to the last user message in the conversation.
//
// Example:
//
//	resp, err := nxuskit.Completion(ctx, "gpt-4o", "What's in this image?",
//	    nxuskit.WithImages(
//	        nxuskit.ImageSource{URL: "https://example.com/image.jpg"},
//	    ),
//	)
func WithImages(images ...ImageSource) Option {
	return func(r *ChatRequest) error {
		if len(images) == 0 {
			return nil
		}

		// Find the last user message and add images to it
		for i := len(r.Messages) - 1; i >= 0; i-- {
			if r.Messages[i].Role == RoleUser {
				msg := r.Messages[i]
				for _, img := range images {
					if img.URL != "" {
						msg = msg.WithImageURL(img.URL)
					} else if img.Base64 != "" {
						msg = msg.WithImageBase64(img.Base64, img.MediaType)
					}
				}
				r.Messages[i] = msg
				return nil
			}
		}

		// No user message found - this shouldn't happen with Completion()
		// but could happen with empty messages slice
		return nil
	}
}

// getTimeoutFromRequest extracts the timeout from request metadata if set via WithTimeout.
func getTimeoutFromRequest(req *ChatRequest) (time.Duration, bool) {
	if req.Metadata == nil {
		return 0, false
	}
	if t, ok := req.Metadata["_convenience_timeout"].(time.Duration); ok {
		return t, true
	}
	return 0, false
}
