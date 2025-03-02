package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
)

func GetArtifactSystem(system string) artifact.ArtifactSystem {
	switch system {
	case "aarch64-linux":
		return artifact.ArtifactSystem_AARCH64_LINUX
	case "aarch64-macos":
		return artifact.ArtifactSystem_AARCH64_MACOS
	case "x86_64-linux":
		return artifact.ArtifactSystem_X86_64_LINUX
	case "x86_64-macos":
		return artifact.ArtifactSystem_X86_64_MACOS
	default:
		return artifact.ArtifactSystem_UNKNOWN_SYSTEM
	}
}
