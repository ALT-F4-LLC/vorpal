package config

import (
	"flag"
	"fmt"
	"os"
	"strings"

	artifactApi "github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
)

type command struct {
	Agent    string
	Artifact string
	Port     int
	Registry string
	Target   artifactApi.ArtifactSystem
	Variable map[string]string
}

func newCommand() (*command, error) {
	startCmd := flag.NewFlagSet("start", flag.ExitOnError)

	var startVariable []string

	startAgent := startCmd.String("agent", "http://localhost:23151", "agent to use")
	startArtifact := startCmd.String("artifact", "", "artifact to use")
	startPort := startCmd.Int("port", 0, "port to listen on")
	startRegistry := startCmd.String("registry", "http://localhost:23151", "registry to use")
	startTarget := startCmd.String("target", GetSystemDefaultStr(), "target system")
	startCmd.Var(newStringSliceValue(&startVariable), "variable", "variables to use (key=value)")

	switch os.Args[1] {
	case "start":
		startCmd.Parse(os.Args[2:])

		if *startAgent == "" {
			return nil, fmt.Errorf("agent is required")
		}

		if *startArtifact == "" {
			return nil, fmt.Errorf("artifact is required")
		}

		if *startPort == 0 {
			return nil, fmt.Errorf("port is required")
		}

		if *startRegistry == "" {
			return nil, fmt.Errorf("registry is required")
		}

		if *startTarget == "" {
			return nil, fmt.Errorf("target is required")
		}

		system, err := GetSystem(*startTarget)
		if err != nil {
			return nil, fmt.Errorf("failed to get system: %w", err)
		}

		variable := make(map[string]string)

		for _, v := range startVariable {
			parts := strings.Split(v, ",")
			for _, part := range parts {
				kv := strings.Split(part, "=")

				if len(kv) != 2 {
					return nil, fmt.Errorf("invalid variable format: %s", part)
				}

				variable[kv[0]] = kv[1]
			}
		}

		return &command{
			Agent:    *startAgent,
			Artifact: *startArtifact,
			Port:     *startPort,
			Registry: *startRegistry,
			Target:   *system,
			Variable: variable,
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
