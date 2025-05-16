package main

import (
	"log"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func main() {
	context := config.GetContext()

	_, err := language.NewGoBuilder("example").
		WithBuildDirectory("cmd/example").
		WithIncludes([]string{"cmd/example", "go.mod", "go.sum"}).
		Build(context)
	if err != nil {
		log.Fatalf("error building: %v", err)
	}

	context.Run()
}
