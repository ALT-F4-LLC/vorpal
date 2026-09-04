module github.com/ALT-F4-LLC/vorpal/sdk/go

// A minor-line floor, not an exact patch: CI reads this file via
// actions/setup-go's go-version-file, so patch releases of 1.26 are picked up
// without a commit. Add a toolchain directive only to require a newer minor.
go 1.26

require (
	github.com/BurntSushi/toml v1.6.0
	github.com/google/uuid v1.6.0
	google.golang.org/grpc v1.81.1
	google.golang.org/protobuf v1.36.12
)

require (
	golang.org/x/net v0.51.0 // indirect
	golang.org/x/sys v0.42.0 // indirect
	golang.org/x/text v0.34.0 // indirect
	google.golang.org/genproto/googleapis/rpc v0.0.0-20260226221140-a57be14db171 // indirect
)
