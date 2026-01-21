package artifact

import (
	"bytes"
	"fmt"
	"text/template"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

type releaseScriptArgs struct {
	Aarch64Darwin string
	Aarch64Linux  string
	BranchName    string
	X8664Darwin   string
	X8664Linux    string
}

const releaseScript = `
git clone \
    --branch {{.BranchName}} \
    --depth 1 \
    git@github.com:ALT-F4-LLC/vorpal.git

pushd vorpal

git fetch --tags
git tag --delete nightly || true
git push origin :refs/tags/nightly || true
gh release delete --yes nightly || true

git tag nightly
git push --tags

gh release create \
    --notes "Nightly builds from main branch." \
    --prerelease \
    --title "nightly" \
    --verify-tag \
    nightly \
    {{.Aarch64Darwin}}.tar.zst \
    {{.Aarch64Linux}}.tar.zst \
    {{.X8664Darwin}}.tar.zst \
    {{.X8664Linux}}.tar.zst`

func BuildVorpalRelease(context *config.ConfigContext) (*string, error) {
	varAarch64Darwin, err := artifact.
		NewArtifactArgument("aarch64-darwin").
		WithRequire().
		Build(context)
	if err != nil {
		return nil, fmt.Errorf("failed to build artifact argument: %w", err)
	}

	varAarch64Linux, err := artifact.
		NewArtifactArgument("aarch64-linux").
		WithRequire().
		Build(context)
	if err != nil {
		return nil, fmt.Errorf("failed to build artifact argument: %w", err)
	}

	varBranchName, err := artifact.
		NewArtifactArgument("branch-name").
		WithRequire().
		Build(context)
	if err != nil {
		return nil, fmt.Errorf("failed to build artifact argument: %w", err)
	}

	varX8664Darwin, err := artifact.
		NewArtifactArgument("x8664-darwin").
		WithRequire().
		Build(context)
	if err != nil {
		return nil, fmt.Errorf("failed to build artifact argument: %w", err)
	}

	varX8664Linux, err := artifact.
		NewArtifactArgument("x8664-linux").
		WithRequire().
		Build(context)
	if err != nil {
		return nil, fmt.Errorf("failed to build artifact argument: %w", err)
	}

	aarch64Darwin, err := context.FetchArtifactAlias(*varAarch64Darwin)
	if err != nil {
		return nil, fmt.Errorf("failed to fetch artifact: %w", err)
	}

	aarch64Linux, err := context.FetchArtifactAlias(*varAarch64Linux)
	if err != nil {
		return nil, fmt.Errorf("failed to fetch artifact: %w", err)
	}

	githubCli, err := artifact.Gh(context)
	if err != nil {
		return nil, fmt.Errorf("failed to get gh: %w", err)
	}

	x8664Darwin, err := context.FetchArtifactAlias(*varX8664Darwin)
	if err != nil {
		return nil, fmt.Errorf("failed to fetch artifact: %w", err)
	}

	x8664Linux, err := context.FetchArtifactAlias(*varX8664Linux)
	if err != nil {
		return nil, fmt.Errorf("failed to fetch artifact: %w", err)
	}

	scriptTemplate, err := template.New("script").Parse(releaseScript)
	if err != nil {
		return nil, fmt.Errorf("failed to parse script template: %w", err)
	}

	var script bytes.Buffer

	scriptVars := releaseScriptArgs{
		Aarch64Darwin: *aarch64Darwin,
		Aarch64Linux:  *aarch64Linux,
		BranchName:    *varBranchName,
		X8664Darwin:   *x8664Darwin,
		X8664Linux:    *x8664Linux,
	}

	if scriptErr := scriptTemplate.Execute(&script, scriptVars); scriptErr != nil {
		return nil, fmt.Errorf("failed to execute script template: %w", scriptErr)
	}

	return artifact.NewTask("vorpal-release", script.String(), SYSTEMS).
		WithArtifacts([]*string{
			aarch64Darwin,
			aarch64Linux,
			githubCli,
			x8664Darwin,
			x8664Linux,
		}).
		Build(context)
}
