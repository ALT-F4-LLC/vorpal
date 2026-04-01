package artifact

import (
	"fmt"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func BuildVorpalWebsite(context *config.ConfigContext) (*string, error) {
	bun, err := artifact.Bun(context)
	if err != nil {
		return nil, fmt.Errorf("failed to build bun: %w", err)
	}

	bunBin := fmt.Sprintf("%s/bin", artifact.GetEnvKey(*bun))

	name := "vorpal-website"

	source := artifact.NewArtifactSource(name, ".").
		WithIncludes([]string{"website"}).
		WithExcludes([]string{
			"website/.astro",
			"website/README.md",
			"website/dist",
			"website/node_modules",
		}).Build()

	stepScript := fmt.Sprintf(`pushd ./source/vorpal-website/website
%s/bun install
%s/bun run build
cp -r dist/* $VORPAL_OUTPUT/
`, bunBin, bunBin)

	step, err := artifact.Shell(
		context,
		[]*string{bun},
		[]string{
			"ASTRO_TELEMETRY_DISABLED=1",
			fmt.Sprintf("PATH=%s", bunBin),
		},
		stepScript,
		nil,
	)
	if err != nil {
		return nil, fmt.Errorf("failed to create shell step: %w", err)
	}

	return artifact.NewArtifact(name, []*api.ArtifactStep{step}, SYSTEMS).
		WithSources([]*api.ArtifactSource{&source}).
		Build(context)
}
