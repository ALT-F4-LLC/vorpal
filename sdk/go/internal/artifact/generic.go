package artifact

import (
	artifactApi "github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/context"
)

func AddArtifact(
	context *context.ConfigContext,
	artifacts []*artifactApi.ArtifactId,
	environments map[string]string,
	name string,
	script string,
	sources []*artifactApi.ArtifactSourceId,
	systems []string,
) (*artifactApi.ArtifactId, error) {
	// 1. Setup target

	target := context.GetTarget()

	// 2. Setup artifacts

	if target == artifactApi.ArtifactSystem_AARCH64_LINUX || target == artifactApi.ArtifactSystem_X86_64_LINUX {
		// TODO: add linux debian and linux vorpal
	}

	// 3. Setup environments

	env := make(map[string]string)

	if target == artifactApi.ArtifactSystem_AARCH64_LINUX || target == artifactApi.ArtifactSystem_X86_64_LINUX {
		env["PATH"] = "/usr/bin:/usr/sbin"
		env["SSL_CERT_FILE"] = "/etc/ssl/certs/ca-certificates.crt"
	}

	if target == artifactApi.ArtifactSystem_AARCH64_MACOS || target == artifactApi.ArtifactSystem_X86_64_MACOS {
		env["PATH"] = "/usr/local/bin:/usr/bin:/usr/sbin:/bin"
	}

	// 3a. Add environment path if defined

	if pathValue, exists := environments["PATH"]; exists {
		env["PATH"] = pathValue + ":" + env["PATH"]
	}

	// 3b. Add environment variables

	for key, value := range environments {
		if key != "PATH" {
			env[key] = value
		}
	}

	// 4. Setup steps

	steps := make([]*artifactApi.ArtifactStep, 0)

	if target == artifactApi.ArtifactSystem_AARCH64_LINUX || target == artifactApi.ArtifactSystem_X86_64_LINUX {
		// TODO: add linux steps
	}

	if target == artifactApi.ArtifactSystem_AARCH64_MACOS || target == artifactApi.ArtifactSystem_X86_64_MACOS {
		stepsBash := Bash(env, &script)
		steps = append(steps, &stepsBash)
	}

	// 5. Add artifact to context

	return context.AddArtifact(name, artifacts, sources, steps, systems)
}
