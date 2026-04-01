package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Gopls(context *config.ConfigContext) (*string, error) {
	name := "gopls"

	source := sourceTools(name)

	systems := []api.ArtifactSystem{
		api.ArtifactSystem_AARCH64_DARWIN,
		api.ArtifactSystem_AARCH64_LINUX,
		api.ArtifactSystem_X8664_DARWIN,
		api.ArtifactSystem_X8664_LINUX,
	}

	sourceDir := fmt.Sprintf("./source/%s", source.Name)

	stepScript := fmt.Sprintf(`pushd %s

mkdir -p $VORPAL_OUTPUT/bin

go build -C %s -o $VORPAL_OUTPUT/bin/%s  .

go clean -modcache`, sourceDir, name, name)

	git, err := Git(context)
	if err != nil {
		return nil, err
	}

	goBin, err := GoBin(context)
	if err != nil {
		return nil, err
	}

	system := context.GetTarget()

	var goarch string
	switch system {
	case api.ArtifactSystem_AARCH64_DARWIN, api.ArtifactSystem_AARCH64_LINUX:
		goarch = "arm64"
	case api.ArtifactSystem_X8664_DARWIN, api.ArtifactSystem_X8664_LINUX:
		goarch = "amd64"
	default:
		return nil, fmt.Errorf("unsupported target system: %s", system)
	}

	var goos string
	switch system {
	case api.ArtifactSystem_AARCH64_DARWIN, api.ArtifactSystem_X8664_DARWIN:
		goos = "darwin"
	case api.ArtifactSystem_AARCH64_LINUX, api.ArtifactSystem_X8664_LINUX:
		goos = "linux"
	default:
		return nil, fmt.Errorf("unsupported target system: %s", system)
	}

	environments := []string{
		fmt.Sprintf("GOARCH=%s", goarch),
		"GOCACHE=$VORPAL_WORKSPACE/go/cache",
		fmt.Sprintf("GOOS=%s", goos),
		"GOPATH=$VORPAL_WORKSPACE/go",
		fmt.Sprintf("PATH=%s/bin", GetEnvKey(*goBin)),
	}

	step, err := Shell(context, []*string{git, goBin}, environments, stepScript, nil)
	if err != nil {
		return nil, err
	}

	return NewArtifact(name, []*api.ArtifactStep{step}, systems).
		WithAliases([]string{fmt.Sprintf("%s:0.42.0", name)}).
		WithSources([]*api.ArtifactSource{&source}).
		Build(context)
}
