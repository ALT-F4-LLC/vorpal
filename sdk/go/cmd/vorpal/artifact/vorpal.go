package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Vorpal(context *config.ConfigContext) (*string, error) {
	return language.NewRust("vorpal", SYSTEMS).
		WithBins([]string{"vorpal"}).
		WithIncludes([]string{"cli", "sdk/rust"}).
		WithPackages([]string{"vorpal-cli", "vorpal-sdk"}).
		Build(context)
}
