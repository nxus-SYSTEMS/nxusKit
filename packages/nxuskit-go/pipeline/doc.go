// Package pipeline provides infrastructure for defining multi-stage LLM workflows
// as portable JSON/YAML configuration files compatible with Peeler.
//
// # Features
//
//   - Pipeline Definitions: Define multi-stage workflows with LLM and CLIPS stages
//   - DAG Validation: Ensure pipeline stages form a valid directed acyclic graph
//   - Format Support: Load pipelines from JSON or YAML files
//   - Peeler Compatibility: Convert between nxusKit and Peeler formats
//
// # Example
//
//	// Load a pipeline from a JSON file
//	p, err := pipeline.LoadPipeline("my-pipeline.json")
//	if err != nil {
//	    log.Fatal(err)
//	}
//
//	// Validate the pipeline DAG
//	if err := p.Validate(); err != nil {
//	    log.Fatal("Invalid pipeline:", err)
//	}
//
//	fmt.Printf("Loaded %d stages\n", len(p.Stages))
package pipeline
