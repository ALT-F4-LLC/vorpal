package config

import (
	"fmt"
	"runtime"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
)

func GetSystemDefaultStr() string {
	goarch := runtime.GOARCH
	goos := runtime.GOOS

	if goarch == "arm64" {
		goarch = "aarch64"
	}

	return fmt.Sprintf("%s-%s", goarch, goos)
}

func GetSystemDefault() (*artifact.ArtifactSystem, error) {
	platform := GetSystemDefaultStr()

	return GetSystem(platform)
}

func GetSystem(system string) (*artifact.ArtifactSystem, error) {
	aarch64Darwin := artifact.ArtifactSystem_AARCH64_DARWIN
	aarch64Linux := artifact.ArtifactSystem_AARCH64_LINUX
	x8664Darwin := artifact.ArtifactSystem_X8664_DARWIN
	x8664Linux := artifact.ArtifactSystem_X8664_LINUX

	switch system {
	case "aarch64-darwin":
		return &aarch64Darwin, nil
	case "aarch64-linux":
		return &aarch64Linux, nil
	case "x86_64-darwin":
		return &x8664Darwin, nil
	case "x86_64-linux":
		return &x8664Linux, nil
	default:
		return nil, fmt.Errorf("unknown system: %s", system)
	}
}
