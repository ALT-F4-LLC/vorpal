package main

import (
	"log"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func main() {
	context := config.GetContext()
	contextArtifact := context.GetArtifactName()

	var err error

	switch contextArtifact {
	case "vorpal":
		_, err = build(context)
	case "vorpal-process":
		_, err = buildProcess(context)
	case "vorpal-release":
		_, err = buildRelease(context)
	case "vorpal-shell":
		_, err = buildShell(context)
	case "vorpal-test":
		_, err = buildTest(context)
	default:
		log.Fatalf("unknown artifact %s", contextArtifact)
	}
	if err != nil {
		log.Fatalf("failed to build artifact %s: %v", contextArtifact, err)
	}

	context.Run()
}
