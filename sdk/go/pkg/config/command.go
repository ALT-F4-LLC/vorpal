package config

import (
	"flag"
	"fmt"
	"os"
	"strings"
)

type command struct {
	Agent             string
	Artifact          string
	ArtifactContext   string
	ArtifactNamespace string
	ArtifactSystem    string
	ArtifactUnlock    bool
	ArtifactVariable  map[string]string
	Port              int
	Registry          string
}

func NewCommand() (*command, error) {
	startCmd := flag.NewFlagSet("start", flag.ExitOnError)

	var startVariable []string

	startAgent := startCmd.String("agent", "http://localhost:23151", "agent to use")
	startArtifact := startCmd.String("artifact", "", "artifact to use")
	startArtifactContext := startCmd.String("artifact-context", "", "artifact context to use")
	startArtifactNamespace := startCmd.String("artifact-namespace", "", "artifact namespace to use")
	startArtifactSystem := startCmd.String("artifact-system", GetSystemDefaultStr(), "system to use")
	startArtifactUnlock := startCmd.Bool("artifact-unlock", false, "unlock lockfile")
	startCmd.Var(newStringSliceValue(&startVariable), "artifact-variable", "variables to use (key=value)")
	startPort := startCmd.Int("port", 0, "port to listen on")
	startRegistry := startCmd.String("registry", "http://localhost:23151", "registry to use")

	switch os.Args[1] {
	case "start":
		startCmd.Parse(os.Args[2:])

		if *startAgent == "" {
			return nil, fmt.Errorf("--agent is required")
		}

		if *startArtifact == "" {
			return nil, fmt.Errorf("--artifact is required")
		}

		if *startArtifactContext == "" {
			return nil, fmt.Errorf("--artifact-context is required")
		}

		if *startArtifactNamespace == "" {
			return nil, fmt.Errorf("--artifact-namespace is required")
		}

		if *startArtifactSystem == "" {
			return nil, fmt.Errorf("--artifact-system is required")
		}

		if *startPort == 0 {
			return nil, fmt.Errorf("--port is required")
		}

		if *startRegistry == "" {
			return nil, fmt.Errorf("--registry is required")
		}

		artifactVariable := make(map[string]string)

		for _, v := range startVariable {
			parts := strings.Split(v, ",")
			for _, part := range parts {
				kv := strings.Split(part, "=")

				if len(kv) != 2 {
					return nil, fmt.Errorf("invalid variable format: %s", part)
				}

				artifactVariable[kv[0]] = kv[1]
			}
		}

		return &command{
			Agent:             *startAgent,
			Artifact:          *startArtifact,
			ArtifactContext:   *startArtifactContext,
			ArtifactNamespace: *startArtifactNamespace,
			ArtifactSystem:    *startArtifactSystem,
			ArtifactUnlock:    *startArtifactUnlock,
			ArtifactVariable:  artifactVariable,
			Port:              *startPort,
			Registry:          *startRegistry,
		}, nil
	default:
		return nil, fmt.Errorf("unknown command")
	}
}

// stringSliceValue implements the flag.Value interface
type stringSliceValue struct {
	values *[]string
}

func newStringSliceValue(p *[]string) *stringSliceValue {
	return &stringSliceValue{values: p}
}

// String returns the string representation of the slice
func (s *stringSliceValue) String() string {
	if s.values == nil || len(*s.values) == 0 {
		return ""
	}
	return fmt.Sprintf("%v", *s.values)
}

// Set appends the value to the slice
func (s *stringSliceValue) Set(value string) error {
	*s.values = append(*s.values, value)
	return nil
}
