package main

import (
	"log"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func main() {
	ctx := config.GetContext()

	log.Printf("Context: %v", ctx)
}
