package config

import (
	"flag"
	"fmt"
	"os"
	"runtime"

	artifactApi "github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
)

type command struct {
	Agent    string
	Port     int
	Registry string
	Target   artifactApi.ArtifactSystem
}

func getDefaultSystem() string {
	arch := runtime.GOARCH
	os := runtime.GOOS

	if arch == "arm64" {
		arch = "aarch64"
	}

	return fmt.Sprintf("%s-%s", arch, os)
}

func NewCommand() (*command, error) {
	startCmd := flag.NewFlagSet("start", flag.ExitOnError)

	startAgent := startCmd.String("agent", "localhost:23151", "agent to use")
	startPort := startCmd.Int("port", 0, "port to listen on")
	startRegistry := startCmd.String("registry", "localhost:23151", "registry to use")
	startTarget := startCmd.String("target", getDefaultSystem(), "target system")

	switch os.Args[1] {
	case "start":
		startCmd.Parse(os.Args[2:])

		if *startAgent == "" {
			return nil, fmt.Errorf("agent is required")
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

		return &command{
			Agent:    *startAgent,
			Port:     *startPort,
			Registry: *startRegistry,
			Target:   *system,
		}, nil
	default:
		return nil, fmt.Errorf("unknown command")
	}
}
