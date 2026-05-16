// Public CE solver placeholder types.
//
// Pro solver domain types are not shipped in public CE source or release bundles.
package nxuskit

type SolverStreamChunk struct {
	Error string `json:"error,omitempty"`
}

type SolverStreamResult struct {
	Chunks []SolverStreamChunk `json:"chunks,omitempty"`
}
