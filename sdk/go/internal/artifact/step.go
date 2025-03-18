package artifact

import (
	"fmt"
	"strings"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
)

func GetArtifactEnvKey(artifact *artifact.ArtifactId) string {
	return fmt.Sprintf("$VORPAL_ARTIFACT_%s", strings.ReplaceAll(strings.ToLower(artifact.Name), "-", "_"))
}

func Bash(environment map[string]string, script *string) artifact.ArtifactStep {
	entrypoint := "bash"

	environments := make([]artifact.ArtifactStepEnvironment, 0)

	path := "/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin"

	for key, value := range environment {
		if key == "PATH" {
			value = value + ":" + path
		}

		environments = append(environments, artifact.ArtifactStepEnvironment{
			Key:   key,
			Value: value,
		})
	}

	return artifact.ArtifactStep{
		Arguments:    []string{},
		Entrypoint:   &entrypoint,
		Environments: nil,
		Script:       script,
	}
}

func Bwrap(arguments []string, artifacts []*artifact.ArtifactId, environment map[string]string, rootfs *artifact.ArtifactId, script string) artifact.ArtifactStep {
	argumentsDefaults := []string{
		"--unshare-all",
		"--share-net",
		"--clearenv",
		"--chdir",
		"$VORPAL_WORKSPACE",
		"--gid",
		"1000",
		"--uid",
		"1000",
		"--dev",
		"/dev",
		"--proc",
		"/proc",
		"--tmpfs",
		"/tmp",
		"--bind",
		"$VORPAL_OUTPUT",
		"$VORPAL_OUTPUT",
		"--bind",
		"$VORPAL_WORKSPACE",
		"$VORPAL_WORKSPACE",
		"--setenv",
		"VORPAL_OUTPUT",
		"$VORPAL_OUTPUT",
		"--setenv",
		"VORPAL_WORKSPACE",
		"$VORPAL_WORKSPACE",
	}

	if rootfs != nil {
		argumentsRootfs := []string{
			// mount bin
			"--ro-bind",
			fmt.Sprintf("%s/bin", rootfs),
			"/bin",
			// mount etc
			"--ro-bind",
			fmt.Sprintf("%s/etc", rootfs),
			"/etc",
			// mount lib
			"--ro-bind",
			fmt.Sprintf("%s/lib", rootfs),
			"/lib",
			// mount lib64 (if exists)
			"--ro-bind-try",
			fmt.Sprintf("%s/lib64", rootfs),
			"/lib64",
			// mount sbin
			"--ro-bind",
			fmt.Sprintf("%s/sbin", rootfs),
			"/sbin",
			// mount usr
			"--ro-bind",
			fmt.Sprintf("%s/usr", rootfs),
			"/usr",
		}

		argumentsDefaults = append(argumentsDefaults, argumentsRootfs...)
	}

	for _, artifact := range artifacts {
		argumentsArtifact := []string{
			"--ro-bind",
			GetArtifactEnvKey(artifact),
			GetArtifactEnvKey(artifact),
		}

		argumentsEnv := []string{
			"--setenv",
			strings.ReplaceAll(GetArtifactEnvKey(artifact), "$", ""),
			GetArtifactEnvKey(artifact),
		}

		argumentsDefaults = append(argumentsDefaults, argumentsArtifact...)
		argumentsDefaults = append(argumentsDefaults, argumentsEnv...)
	}

	for key, value := range environment {
		argumentsEnv := []string{
			"--setenv",
			key,
			value,
		}

		argumentsDefaults = append(argumentsDefaults, argumentsEnv...)
	}

	for _, arg := range arguments {
		argumentsDefaults = append(argumentsDefaults, arg)
	}

	path := "/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin"

	entrypoint := "bwrap"

	return artifact.ArtifactStep{
		Arguments:  argumentsDefaults,
		Entrypoint: &entrypoint,
		Environments: []*artifact.ArtifactStepEnvironment{
			{
				Key:   "PATH",
				Value: path,
			},
		},
		Script: &script,
	}
}

func Docker(arguments []string) artifact.ArtifactStep {
	entrypoint := "docker"

	path := "/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin"

	return artifact.ArtifactStep{
		Arguments:  arguments,
		Entrypoint: &entrypoint,
		Environments: []*artifact.ArtifactStepEnvironment{
			{
				Key:   "PATH",
				Value: path,
			},
		},
		Script: nil,
	}
}
