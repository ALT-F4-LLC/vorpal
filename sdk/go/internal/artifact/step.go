package artifact

import (
	"bytes"
	"fmt"
	"strings"
	"text/template"

	artifactApi "github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

type BashScriptTemplateArgs struct {
	Script string
}

type BwrapScriptTemplateArgs struct {
	Script string
}

const BashScriptTemplate = `#!/bin/bash
set -euo pipefail
{{.Script}}
`

const BwrapScriptTemplate = `#!/bin/bash
set -euo pipefail
{{.Script}}
`

func Bash(
	context *config.ConfigContext,
	artifacts []*string,
	environments []string,
	script *string,
	systems []artifactApi.ArtifactSystem,
) (*artifactApi.ArtifactStep, error) {
	stepEnvironments := make([]string, 0)

	stepEntrypoint := "bash"

	for _, value := range environments {
		if strings.Contains(value, "PATH=") {
			continue
		}

		stepEnvironments = append(stepEnvironments, value)
	}

	stepPathBins := make([]string, 0)

	for _, art := range artifacts {
		stepPathBins = append(stepPathBins, fmt.Sprintf("%s/bin", GetEnvKey(art)))
	}

	stepPathDefault := "/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin"

	stepPath := fmt.Sprintf("%s:%s", strings.Join(stepPathBins, ":"), stepPathDefault)

	for _, value := range environments {
		if strings.Contains(value, "PATH=") {
			stepPath = fmt.Sprintf("%s:%s", strings.ReplaceAll(value, "PATH=", ""), stepPath)
		}
	}

	stepEnvironments = append(stepEnvironments, fmt.Sprintf("PATH=%s", stepPath))

	scriptTemplate, err := template.New("script").Parse(BashScriptTemplate)
	if err != nil {
		return nil, err
	}

	var scriptBuffer bytes.Buffer

	scriptTemplateVars := BashScriptTemplateArgs{
		Script: *script,
	}

	if err := scriptTemplate.Execute(&scriptBuffer, scriptTemplateVars); err != nil {
		return nil, err
	}

	step := NewArtifactStepBuilder()

	step = step.WithArtifacts(artifacts, systems)
	step = step.WithEntrypoint(stepEntrypoint, systems)
	step = step.WithEnvironments(stepEnvironments, systems)
	step = step.WithScript(scriptBuffer.String(), systems)

	return step.Build(context), nil
}

func Bwrap(
	context *config.ConfigContext,
	arguments []string,
	artifacts []*string,
	environments []string,
	rootfs *string,
	script string,
	systems []artifactApi.ArtifactSystem,
) (*artifactApi.ArtifactStep, error) {
	// Setup arguments

	stepArguments := []string{
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

	// Setup artifacts arguments

	stepArtifacts := make([]*string, 0)

	if rootfs != nil {
		rootfsArgs := []string{
			"--ro-bind",
			fmt.Sprintf("%s/bin", GetEnvKey(rootfs)),
			"/bin",
			"--ro-bind",
			fmt.Sprintf("%s/etc", GetEnvKey(rootfs)),
			"/etc",
			"--ro-bind",
			fmt.Sprintf("%s/lib", GetEnvKey(rootfs)),
			"/lib",
			"--ro-bind-try",
			fmt.Sprintf("%s/lib64", GetEnvKey(rootfs)),
			"/lib64",
			"--ro-bind",
			fmt.Sprintf("%s/sbin", GetEnvKey(rootfs)),
			"/sbin",
			"--ro-bind",
			fmt.Sprintf("%s/usr", GetEnvKey(rootfs)),
			"/usr",
		}

		stepArguments = append(stepArguments, rootfsArgs...)
		stepArtifacts = append(stepArtifacts, rootfs)
	}

	for _, artifact := range artifacts {
		stepArtifacts = append(stepArtifacts, artifact)
	}

	for _, art := range stepArtifacts {
		stepArguments = append(stepArguments, "--ro-bind")
		stepArguments = append(stepArguments, GetEnvKey(art))
		stepArguments = append(stepArguments, GetEnvKey(art))
		stepArguments = append(stepArguments, "--setenv")
		stepArguments = append(stepArguments, strings.ReplaceAll(GetEnvKey(art), "$", ""))
		stepArguments = append(stepArguments, GetEnvKey(art))
	}

	// Setup environment arguments

	stepPathBins := make([]string, 0)

	for _, art := range stepArtifacts {
		stepPathBins = append(stepPathBins, fmt.Sprintf("%s/bin", GetEnvKey(art)))
	}

	stepPath := fmt.Sprintf("%s:%s", strings.Join(stepPathBins, ":"), "/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin")

	for _, envvar := range environments {
		if strings.Contains(envvar, "PATH=") {
			stepPath = fmt.Sprintf("%s:%s", strings.ReplaceAll(envvar, "PATH=", ""), stepPath)
		}
	}

	stepArguments = append(stepArguments, "--setenv")
	stepArguments = append(stepArguments, "PATH")
	stepArguments = append(stepArguments, stepPath)

	for _, envvar := range environments {
		key := strings.Split(envvar, "=")[0]
		value := strings.Split(envvar, "=")[1]

		if strings.Contains(key, "PATH") {
			continue
		}

		stepArguments = append(stepArguments, "--setenv")
		stepArguments = append(stepArguments, key)
		stepArguments = append(stepArguments, value)
	}

	// Setup arguments

	for _, argument := range arguments {
		stepArguments = append(stepArguments, argument)
	}

	// Setup script

	scriptTemplate, err := template.New("script").Parse(BwrapScriptTemplate)
	if err != nil {
		return nil, err
	}

	var scriptBuffer bytes.Buffer

	scriptTemplateVars := BwrapScriptTemplateArgs{
		Script: script,
	}

	if err := scriptTemplate.Execute(&scriptBuffer, scriptTemplateVars); err != nil {
		return nil, err
	}

	// Setup step

	step := NewArtifactStepBuilder()

	step = step.WithArguments(stepArguments, systems)
	step = step.WithArtifacts(stepArtifacts, systems)
	step = step.WithEntrypoint("bwrap", systems)
	step = step.WithEnvironments([]string{"PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin"}, systems)
	step = step.WithScript(scriptBuffer.String(), systems)

	return step.Build(context), nil
}

func Shell(
	context *config.ConfigContext,
	artifacts []*string,
	environments []string,
	script string,
) (*artifactApi.ArtifactStep, error) {
	stepTarget := context.GetTarget()

	if stepTarget == artifactApi.ArtifactSystem_AARCH64_DARWIN || stepTarget == artifactApi.ArtifactSystem_X8664_DARWIN {
		return Bash(
			context,
			artifacts,
			environments,
			&script,
			[]artifactApi.ArtifactSystem{
				artifactApi.ArtifactSystem_AARCH64_DARWIN,
				artifactApi.ArtifactSystem_X8664_DARWIN,
			},
		)
	}

	if stepTarget == artifactApi.ArtifactSystem_AARCH64_LINUX || stepTarget == artifactApi.ArtifactSystem_X8664_LINUX {
		linux_vorpal, err := context.FetchArtifact("79958083229520a9e8cbf94ed5f6da40c3bf98ae666b3e20c01bc272fd92c2bb")
		if err != nil {
			return nil, err
		}

		return Bwrap(
			context,
			[]string{},
			artifacts,
			environments,
			linux_vorpal,
			script,
			[]artifactApi.ArtifactSystem{
				artifactApi.ArtifactSystem_AARCH64_LINUX,
				artifactApi.ArtifactSystem_X8664_LINUX,
			},
		)
	}

	return nil, fmt.Errorf("unsupported shell step system: %s", stepTarget)
}

func Docker(
	context *config.ConfigContext,
	arguments []string,
	artifacts []*string,
	systems []artifactApi.ArtifactSystem,
) *artifactApi.ArtifactStep {
	step := NewArtifactStepBuilder()

	step = step.WithArguments(arguments, systems)
	step = step.WithArtifacts(artifacts, systems)
	step = step.WithEntrypoint("docker", systems)
	step = step.WithEnvironments([]string{"PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin"}, systems)

	return step.Build(context)
}
