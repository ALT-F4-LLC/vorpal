package cli

import (
	"flag"
	"fmt"
	"os"
	"runtime"

	_artifact "github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/artifact"
)

type startCommand struct {
	Level    string
	Port     int
	Registry string
	Target   _artifact.ArtifactSystem
}

func getDefaultSystem() string {
	arch := runtime.GOARCH
	os := runtime.GOOS

	if arch == "arm64" {
		arch = "aarch64"
	}

	if os == "darwin" {
		os = "macos"
	}

	return fmt.Sprintf("%s-%s", arch, os)
}

func NewStartCommand() (*startCommand, error) {
	startCmd := flag.NewFlagSet("start", flag.ExitOnError)

	startLevel := startCmd.String("level", "INFO", "logging level")
	startPort := startCmd.Int("port", 0, "port to listen on")
	startRegistry := startCmd.String("registry", "http://localhost:23151", "registry to use")
	startTarget := startCmd.String("target", getDefaultSystem(), "target system")

	switch os.Args[1] {
	case "start":
		startCmd.Parse(os.Args[2:])

		if *startPort == 0 {
			return nil, fmt.Errorf("port is required")
		}

		if *startRegistry == "" {
			return nil, fmt.Errorf("registry is required")
		}

		if *startTarget == "" {
			return nil, fmt.Errorf("target is required")
		}

		system := artifact.GetArtifactSystem(*startTarget)

		if system == _artifact.ArtifactSystem_UNKNOWN_SYSTEM {
			return nil, fmt.Errorf("unknown target system")
		}

		return &startCommand{
			Level:    *startLevel,
			Port:     *startPort,
			Registry: *startRegistry,
			Target:   system,
		}, nil
	default:
		return nil, fmt.Errorf("unknown command")
	}
}
