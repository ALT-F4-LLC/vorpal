package main

import (
	"log"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func main() {
	ctx := config.GetContext()

	// -> 1. Activate: `source "$(vorpal build --path 'example-dev')/bin/activate"`
	// -> 2. Deactivate: `deactivate`

	_, err := language.NewGoDevelopmentEnvironment("example-dev", config.SYSTEMS).
		Build(ctx)
	if err != nil {
		log.Fatalf("error building development environment: %v", err)
	}

	// -> 1. Build: `vorpal build 'example'`
	// -> 2. Run: `$(vorpal build --path 'example')/bin/example`

	_, err = language.NewGo("example", config.SYSTEMS).
		WithBuildDirectory("cmd/example").
		WithIncludes([]string{"cmd/example", "go.mod", "go.sum"}).
		Build(ctx)
	if err != nil {
		log.Fatalf("error building: %v", err)
	}

	ctx.Run()
}
