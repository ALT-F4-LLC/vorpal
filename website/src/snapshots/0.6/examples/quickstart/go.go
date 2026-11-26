package main

import (
	"log"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func main() {
	ctx := config.GetContext()

	// -> 1. Activate: `source "$(vorpal build --path 'hello-world-dev')/bin/activate"`
	// -> 2. Deactivate: `deactivate`

	_, err := language.NewGoDevelopmentEnvironment("hello-world-dev", config.SYSTEMS).
		Build(ctx)
	if err != nil {
		log.Fatalf("error building development environment: %v", err)
	}

	// -> 1. Build: `vorpal build 'hello-world'`
	// -> 2. Run: `$(vorpal build --path 'hello-world')/bin/hello-world`

	_, err = language.NewGo("hello-world", config.SYSTEMS).
		WithBuildDirectory("cmd/hello-world").
		WithIncludes([]string{"cmd/hello-world", "go.mod", "go.sum"}).
		Build(ctx)
	if err != nil {
		log.Fatalf("error building: %v", err)
	}

	ctx.Run()
}
