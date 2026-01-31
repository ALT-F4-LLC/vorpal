package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func LinuxVorpalSlim(ctx *config.ConfigContext) (*string, error) {
	return ctx.FetchArtifactAlias("linux-vorpal-slim:latest")
}
