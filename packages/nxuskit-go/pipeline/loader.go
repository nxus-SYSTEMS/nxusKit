// Package pipeline provides loading from JSON and YAML files.
package pipeline

import (
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"

	"gopkg.in/yaml.v3"
)

// LoadError represents errors that can occur during pipeline loading.
type LoadError struct {
	Code    string
	Message string
	Cause   error
}

func (e *LoadError) Error() string {
	if e.Cause != nil {
		return fmt.Sprintf("%s: %v", e.Message, e.Cause)
	}
	return e.Message
}

func (e *LoadError) Unwrap() error {
	return e.Cause
}

var (
	// ErrUnsupportedFormat indicates an unsupported file format.
	ErrUnsupportedFormat = errors.New("unsupported file format")
	// ErrLicenseRequired indicates Pro license is required for CLIPS features.
	ErrLicenseRequired = errors.New("pro license required for CLIPS features")
)

// fileFormat represents the detected file format.
type fileFormat int

const (
	formatJSON fileFormat = iota
	formatYAML
)

// detectFormat determines the file format from the extension.
func detectFormat(path string) (fileFormat, error) {
	ext := strings.ToLower(filepath.Ext(path))
	switch ext {
	case ".json":
		return formatJSON, nil
	case ".yaml", ".yml":
		return formatYAML, nil
	default:
		return 0, &LoadError{
			Code:    "UNSUPPORTED_FORMAT",
			Message: fmt.Sprintf("unsupported file format: %s. Expected .json or .yaml/.yml", ext),
		}
	}
}

// LoadPipeline loads a pipeline definition from a file.
// Automatically detects JSON or YAML format based on file extension.
func LoadPipeline(path string) (*PipelineDefinition, error) {
	slog.Info("Loading pipeline definition", "path", path)

	format, err := detectFormat(path)
	if err != nil {
		return nil, err
	}

	data, err := os.ReadFile(path)
	if err != nil {
		return nil, &LoadError{
			Code:    "IO_ERROR",
			Message: "failed to read file",
			Cause:   err,
		}
	}

	pipeline, err := LoadPipelineFromBytes(data, format)
	if err != nil {
		return nil, err
	}

	slog.Info("Pipeline loaded successfully",
		"pipeline_id", pipeline.ID,
		"pipeline_name", pipeline.Name,
		"stage_count", len(pipeline.Stages),
	)

	return pipeline, nil
}

// LoadPipelineFromBytes loads a pipeline definition from byte content.
func LoadPipelineFromBytes(data []byte, format fileFormat) (*PipelineDefinition, error) {
	var pipeline PipelineDefinition

	switch format {
	case formatJSON:
		if err := json.Unmarshal(data, &pipeline); err != nil {
			return nil, &LoadError{
				Code:    "JSON_ERROR",
				Message: "failed to parse JSON",
				Cause:   err,
			}
		}
	case formatYAML:
		if err := yaml.Unmarshal(data, &pipeline); err != nil {
			return nil, &LoadError{
				Code:    "YAML_ERROR",
				Message: "failed to parse YAML",
				Cause:   err,
			}
		}
	}

	// Check for CLIPS stages that require Pro license
	if err := checkClipsLicense(&pipeline); err != nil {
		return nil, err
	}

	return &pipeline, nil
}

// checkClipsLicense checks if CLIPS stages are present and license is available.
// In the free tier, this returns an error for CLIPS stages.
func checkClipsLicense(pipeline *PipelineDefinition) error {
	// This is a stub - actual license checking would be implemented
	// based on the licensing system
	for _, stage := range pipeline.Stages {
		switch stage.Type {
		case StageTypeClipsEval, StageTypeClipsGen, StageTypeHybrid:
			// In production, check for actual Pro license
			// For now, we allow all stages (license check returns true)
			// Uncomment below to enforce license:
			// slog.Warn("CLIPS stage detected but Pro license not verified",
			// 	"stage_id", stage.ID,
			// 	"stage_type", stage.Type,
			// )
			// return ErrLicenseRequired
		}
	}
	return nil
}

// LoadPipelineFromJSON loads a pipeline definition from a JSON string.
func LoadPipelineFromJSON(content string) (*PipelineDefinition, error) {
	return LoadPipelineFromBytes([]byte(content), formatJSON)
}

// LoadPipelineFromYAML loads a pipeline definition from a YAML string.
func LoadPipelineFromYAML(content string) (*PipelineDefinition, error) {
	return LoadPipelineFromBytes([]byte(content), formatYAML)
}

// SavePipeline saves a pipeline definition to a file.
// Automatically detects JSON or YAML format based on file extension.
func SavePipeline(pipeline *PipelineDefinition, path string) error {
	slog.Info("Saving pipeline definition", "path", path)

	format, err := detectFormat(path)
	if err != nil {
		return err
	}

	var data []byte
	switch format {
	case formatJSON:
		data, err = json.MarshalIndent(pipeline, "", "  ")
	case formatYAML:
		data, err = yaml.Marshal(pipeline)
	}

	if err != nil {
		return &LoadError{
			Code:    "SERIALIZE_ERROR",
			Message: "failed to serialize pipeline",
			Cause:   err,
		}
	}

	if err := os.WriteFile(path, data, 0644); err != nil {
		return &LoadError{
			Code:    "IO_ERROR",
			Message: "failed to write file",
			Cause:   err,
		}
	}

	slog.Info("Pipeline saved successfully", "path", path)
	return nil
}

// PipelineToJSON serializes a pipeline definition to JSON.
func PipelineToJSON(pipeline *PipelineDefinition) (string, error) {
	data, err := json.MarshalIndent(pipeline, "", "  ")
	if err != nil {
		return "", err
	}
	return string(data), nil
}

// PipelineToYAML serializes a pipeline definition to YAML.
func PipelineToYAML(pipeline *PipelineDefinition) (string, error) {
	data, err := yaml.Marshal(pipeline)
	if err != nil {
		return "", err
	}
	return string(data), nil
}
