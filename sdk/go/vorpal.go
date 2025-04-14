package main

import (
	"bytes"
	"text/template"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/artifact/language"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

type ReleaseScriptTemplateArgs struct {
	BranchName    string
	Aarch64Darwin string
	// Aarch64Linux  string
	// X8664Darwin   string
	// X8664Linux    string
}

const ReleaseScriptTemplate = `
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
    /var/lib/vorpal/store/{{.Aarch64Darwin}}.tar.zst`

func NewVorpal(context *config.ConfigContext) error {
	protoc, err := artifact.Protoc(context)
	if err != nil {
		return err
	}

	artifacts := []*string{
		protoc,
	}

	excludes := []string{
		".cargo",
		".env",
		".envrc",
		".github",
		".gitignore",
		".packer",
		".vagrant",
		"Dockerfile",
		"Vagrantfile",
		"dist",
		"makefile",
		"script",
		"sdk/go",
		"shell.nix",
		"vendor",
		"vorpal-config",
		"vorpal-domains.svg",
		"vorpal-purpose.jpg",
	}

	_, err = language.NewRustBuilder("vorpal").
		WithArtifacts(artifacts).
		WithExcludes(excludes).
		Build(context)
	if err != nil {
		return err
	}

	return nil
}

func NewVorpalShell(context *config.ConfigContext) error {
	gh, err := artifact.Gh(context)
	if err != nil {
		return err
	}

	gobin, err := artifact.GoBin(context)
	if err != nil {
		return err
	}

	goimports, err := artifact.Goimports(context)
	if err != nil {
		return err
	}

	gopls, err := artifact.Gopls(context)
	if err != nil {
		return err
	}

	grpcurl, err := artifact.Grpcurl(context)
	if err != nil {
		return err
	}

	protoc, err := artifact.Protoc(context)
	if err != nil {
		return err
	}

	protocGenGo, err := artifact.ProtocGenGo(context)
	if err != nil {
		return err
	}

	protocGenGoGRPC, err := artifact.ProtocGenGoGRPC(context)
	if err != nil {
		return err
	}

	artifacts := []*string{
		gh,
		gobin,
		goimports,
		gopls,
		grpcurl,
		protoc,
		protocGenGo,
		protocGenGoGRPC,
	}

	_, err = language.NewRustShellBuilder("vorpal-shell").
		WithArtifacts(artifacts).
		Build(context)
	if err != nil {
		return err
	}

	return nil
}

func NewVorpalRelease(context *config.ConfigContext) error {
	varAarch64Darwin, err := artifact.
		NewVariableBuilder("aarch64-darwin").
		WithRequire().
		Build(context)
	if err != nil {
		return err
	}

	// varAarch64Linux, err := artifact.
	// 	NewVariableBuilder("aarch64-linux").
	// 	WithRequire().
	// 	Build(context)
	// if err != nil {
	// 	return err
	// }

	// varX8664Darwin, err := artifact.
	// 	NewVariableBuilder("x8664-darwin").
	// 	WithRequire().
	// 	Build(context)
	// if err != nil {
	// 	return err
	// }

	// varX8664Linux, err := artifact.
	// 	NewVariableBuilder("x8664-linux").
	// 	WithRequire().
	// 	Build(context)
	// if err != nil {
	// 	return err
	// }

	varBranchName, err := artifact.
		NewVariableBuilder("branch-name").
		WithRequire().
		Build(context)
	if err != nil {
		return err
	}

	aarch64Darwin, err := context.FetchArtifact(*varAarch64Darwin)
	if err != nil {
		return err
	}

	// aarch64Linux, err := context.FetchArtifact(*varAarch64Linux)
	// if err != nil {
	// 	return err
	// }

	// x8664Darwin, err := context.FetchArtifact(*varX8664Darwin)
	// if err != nil {
	// 	return err
	// }

	// x8664Linux, err := context.FetchArtifact(*varX8664Linux)
	// if err != nil {
	// 	return err
	// }

	gh, err := artifact.Gh(context)
	if err != nil {
		return err
	}

	artifacts := []*string{
		gh,
		aarch64Darwin,
		// aarch64Linux,
		// x8664Darwin,
		// x8664Linux,
	}

	scriptTemplate, err := template.New("script").Parse(ReleaseScriptTemplate)
	if err != nil {
		return err
	}

	var scriptBuffer bytes.Buffer

	scriptTemplateVars := ReleaseScriptTemplateArgs{
		BranchName:    *varBranchName,
		Aarch64Darwin: *aarch64Darwin,
		// Aarch64Linux:  *aarch64Linux,
		// X8664Darwin:   *x8664Darwin,
		// X8664Linux:    *x8664Linux,
	}

	if err := scriptTemplate.Execute(&scriptBuffer, scriptTemplateVars); err != nil {
		return err
	}

	_, err = artifact.NewArtifactTaskBuilder("vorpal-release", scriptBuffer.String()).
		WithArtifacts(artifacts).
		Build(context)
	if err != nil {
		return err
	}

	return nil
}
