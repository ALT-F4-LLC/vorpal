package artifact

import (
	"bytes"
	"fmt"
	"strings"
	"text/template"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
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
	artifacts []*string,
	environments []string,
	script string,
	secrets []*api.ArtifactStepSecret,
) (*api.ArtifactStep, error) {
	stepEnvironments := make([]string, 0)

	stepEntrypoint := "bash"

	for _, value := range environments {
		if strings.HasPrefix(value, "PATH=") {
			continue
		}

		stepEnvironments = append(stepEnvironments, value)
	}

	stepPathBins := make([]string, 0)

	for _, art := range artifacts {
		if art != nil {
			stepPathBins = append(stepPathBins, fmt.Sprintf("%s/bin", GetEnvKey(*art)))
		}
	}

	stepPathDefault := "/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin"

	stepPath := fmt.Sprintf("%s:%s", strings.Join(stepPathBins, ":"), stepPathDefault)

	for _, value := range environments {
		if pathValue, ok := strings.CutPrefix(value, "PATH="); ok {
			stepPath = fmt.Sprintf("%s:%s", pathValue, stepPath)
		}
	}

	stepEnvironments = append(stepEnvironments, "HOME=$VORPAL_WORKSPACE")
	stepEnvironments = append(stepEnvironments, fmt.Sprintf("PATH=%s", stepPath))

	scriptTemplate, err := template.New("script").Parse(BashScriptTemplate)
	if err != nil {
		return nil, err
	}

	var scriptBuffer bytes.Buffer

	scriptTemplateVars := BashScriptTemplateArgs{
		Script: script,
	}

	if err := scriptTemplate.Execute(&scriptBuffer, scriptTemplateVars); err != nil {
		return nil, err
	}

	step := NewArtifactStep(stepEntrypoint)

	step = step.WithArtifacts(artifacts)
	step = step.WithEnvironments(stepEnvironments)
	step = step.WithScript(scriptBuffer.String())
	step = step.WithSecrets(secrets)

	return step.Build(), nil
}

func Bwrap(
	arguments []string,
	artifacts []*string,
	environments []string,
	rootfs *string,
	script string,
	secrets []*api.ArtifactStepSecret,
) (*api.ArtifactStep, error) {
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
		"--setenv",
		"HOME",
		"$VORPAL_WORKSPACE",
	}

	// Setup artifacts arguments

	stepArtifacts := make([]*string, 0)

	if rootfs != nil {
		rootfsArgs := []string{
			"--ro-bind",
			fmt.Sprintf("%s/bin", GetEnvKey(*rootfs)),
			"/bin",
			"--ro-bind",
			fmt.Sprintf("%s/etc", GetEnvKey(*rootfs)),
			"/etc",
			"--ro-bind",
			fmt.Sprintf("%s/lib", GetEnvKey(*rootfs)),
			"/lib",
			"--ro-bind-try",
			fmt.Sprintf("%s/lib64", GetEnvKey(*rootfs)),
			"/lib64",
			"--ro-bind",
			fmt.Sprintf("%s/sbin", GetEnvKey(*rootfs)),
			"/sbin",
			"--ro-bind",
			fmt.Sprintf("%s/usr", GetEnvKey(*rootfs)),
			"/usr",
		}

		stepArguments = append(stepArguments, rootfsArgs...)
		stepArtifacts = append(stepArtifacts, rootfs)
	}

	for _, artifact := range artifacts {
		stepArtifacts = append(stepArtifacts, artifact)
	}

	for _, art := range stepArtifacts {
		if art != nil {
			stepArguments = append(stepArguments, "--ro-bind")
			stepArguments = append(stepArguments, GetEnvKey(*art))
			stepArguments = append(stepArguments, GetEnvKey(*art))
			stepArguments = append(stepArguments, "--setenv")
			stepArguments = append(stepArguments, strings.ReplaceAll(GetEnvKey(*art), "$", ""))
			stepArguments = append(stepArguments, GetEnvKey(*art))
		}
	}

	// Setup environment arguments

	stepPathBins := make([]string, 0)

	for _, art := range stepArtifacts {
		if art != nil {
			stepPathBins = append(stepPathBins, fmt.Sprintf("%s/bin", GetEnvKey(*art)))
		}
	}

	stepPath := fmt.Sprintf("%s:%s", strings.Join(stepPathBins, ":"), "/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin")

	for _, envvar := range environments {
		if pathValue, ok := strings.CutPrefix(envvar, "PATH="); ok {
			stepPath = fmt.Sprintf("%s:%s", pathValue, stepPath)
		}
	}

	stepArguments = append(stepArguments, "--setenv")
	stepArguments = append(stepArguments, "PATH")
	stepArguments = append(stepArguments, stepPath)

	for _, envvar := range environments {
		parts := strings.SplitN(envvar, "=", 2)
		if len(parts) != 2 {
			continue
		}

		key := parts[0]
		value := parts[1]

		if key == "PATH" {
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

	step := NewArtifactStep("bwrap")

	step = step.WithArguments(stepArguments)
	step = step.WithArtifacts(stepArtifacts)
	step = step.WithEnvironments([]string{"PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin"})
	step = step.WithScript(scriptBuffer.String())
	step = step.WithSecrets(secrets)

	return step.Build(), nil
}

func Shell(
	context *config.ConfigContext,
	artifacts []*string,
	environments []string,
	script string,
	secrets []*api.ArtifactStepSecret,
) (*api.ArtifactStep, error) {
	stepSystem := context.GetTarget()

	if stepSystem == api.ArtifactSystem_AARCH64_DARWIN || stepSystem == api.ArtifactSystem_X8664_DARWIN {
		return Bash(
			artifacts,
			environments,
			script,
			secrets,
		)
	}

	if stepSystem == api.ArtifactSystem_AARCH64_LINUX || stepSystem == api.ArtifactSystem_X8664_LINUX {
		linux_vorpal, err := context.FetchArtifactAlias("library/linux-vorpal:latest")
		if err != nil {
			return nil, err
		}

		return Bwrap(
			[]string{},
			artifacts,
			environments,
			linux_vorpal,
			script,
			secrets,
		)
	}

	return nil, fmt.Errorf("unsupported shell step system: %s", stepSystem)
}

// TODO: Add support for secrets with docker step

func Docker(
	arguments []string,
	artifacts []*string,
) (*api.ArtifactStep, error) {
	step := NewArtifactStep("docker")

	step = step.WithArguments(arguments)
	step = step.WithArtifacts(artifacts)
	step = step.WithEnvironments([]string{"PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin"})

	return step.Build(), nil
}
