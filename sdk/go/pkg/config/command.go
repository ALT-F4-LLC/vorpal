package config

import (
	"flag"
	"fmt"
	"os"
	"strings"
)

type command struct {
	Agent           string
	Artifact        string
	ArtifactContext string
	Lockfile        string
	Update          bool
	Port            int
	Registry        string
	System          string
	Variable        map[string]string
}

func NewCommand() (*command, error) {
	startCmd := flag.NewFlagSet("start", flag.ExitOnError)

	var startVariable []string

	startAgent := startCmd.String("agent", "http://localhost:23151", "agent to use")
	startArtifact := startCmd.String("artifact", "", "artifact to use")
	startArtifactContext := startCmd.String("artifact-context", "", "artifact context to use")
	startCmd.Var(newStringSliceValue(&startVariable), "variable", "variables to use (key=value)")
	startLockfile := startCmd.String("lockfile", "Vorpal.lock", "lockfile to use")
	startPort := startCmd.Int("port", 0, "port to listen on")
	startRegistry := startCmd.String("registry", "http://localhost:23151", "registry to use")
	startSystem := startCmd.String("system", GetSystemDefaultStr(), "system to use")
	startUpdate := startCmd.Bool("update", false, "update lockfile")

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

		if *startLockfile == "" {
			return nil, fmt.Errorf("--lockfile is required")
		}

		if *startPort == 0 {
			return nil, fmt.Errorf("--port is required")
		}

		if *startRegistry == "" {
			return nil, fmt.Errorf("--registry is required")
		}

		if *startSystem == "" {
			return nil, fmt.Errorf("--system is required")
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
			Agent:           *startAgent,
			Artifact:        *startArtifact,
			ArtifactContext: *startArtifactContext,
			Lockfile:        *startLockfile,
			Port:            *startPort,
			Registry:        *startRegistry,
			System:          *startSystem,
			Update:          *startUpdate,
			Variable:        variable,
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
