package main

import (
	"errors"
	"log"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func main() {
	context := config.GetContext()

	var err error

	switch context.GetArtifactName() {
	case "vorpal-shell":
		err = NewVorpalShell(context)
	case "vorpal":
		err = NewVorpal(context)
	case "vorpal-release":
		err = NewVorpalRelease(context)
	default:
		err = errors.New("unknown artifact name")
	}

	if err != nil {
		log.Fatalf("failed to create artifact: %v", err)
	}

	context.Run()
}
