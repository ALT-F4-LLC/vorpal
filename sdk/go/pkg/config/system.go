package config

import (
	"fmt"
	"runtime"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
)

type ArtifactSystemInput interface {
	string | api.ArtifactSystem
}

func GetSystemDefaultStr() string {
	goarch := runtime.GOARCH
	goos := runtime.GOOS

	if goarch == "amd64" {
		goarch = "x86_64"
	}

	if goarch == "arm64" {
		goarch = "aarch64"
	}

	return fmt.Sprintf("%s-%s", goarch, goos)
}

func GetSystemDefault() (*api.ArtifactSystem, error) {
	platform := GetSystemDefaultStr()

	return GetSystem(platform)
}

func GetSystem(system string) (*api.ArtifactSystem, error) {
	switch system {
	case "aarch64-darwin":
		system := api.ArtifactSystem_AARCH64_DARWIN
		return &system, nil
	case "aarch64-linux":
		system := api.ArtifactSystem_AARCH64_LINUX
		return &system, nil
	case "x86_64-darwin":
		system := api.ArtifactSystem_X8664_DARWIN
		return &system, nil
	case "x86_64-linux":
		system := api.ArtifactSystem_X8664_LINUX
		return &system, nil
	}

	return nil, fmt.Errorf("unsupported system: %s", system)
}

func NormalizeSystems[T ArtifactSystemInput](systems []T) ([]api.ArtifactSystem, error) {
	artifactSystems := make([]api.ArtifactSystem, len(systems))

	for i, system := range systems {
		switch value := any(system).(type) {
		case string:
			artifactSystem, err := GetSystem(value)
			if err != nil {
				return nil, err
			}
			artifactSystems[i] = *artifactSystem
		case api.ArtifactSystem:
			switch value {
			case api.ArtifactSystem_AARCH64_DARWIN,
				api.ArtifactSystem_AARCH64_LINUX,
				api.ArtifactSystem_X8664_DARWIN,
				api.ArtifactSystem_X8664_LINUX:
				artifactSystems[i] = value
			default:
				return nil, fmt.Errorf("unsupported system: %s", value)
			}
		}
	}

	return artifactSystems, nil
}

func GetSystems(systems ...string) ([]api.ArtifactSystem, error) {
	return NormalizeSystems(systems)
}
