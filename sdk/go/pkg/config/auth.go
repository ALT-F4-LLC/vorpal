package config

import (
	"context"
	"fmt"
	"os"
	"strings"

	"google.golang.org/grpc"
	"google.golang.org/grpc/metadata"
)

// GetServiceSecretPath returns the path to the service authentication secret
func GetServiceSecretPath() string {
	return "/var/lib/vorpal/key/service.secret"
}

// LoadServiceSecret loads the service authentication secret from the standard location
func LoadServiceSecret() (string, error) {
	secretPath := GetServiceSecretPath()

	if _, err := os.Stat(secretPath); os.IsNotExist(err) {
		return "", fmt.Errorf("service secret not found - run 'vorpal system keys generate'")
	}

	secretBytes, err := os.ReadFile(secretPath)
	if err != nil {
		return "", fmt.Errorf("failed to read service secret: %v", err)
	}

	secret := strings.TrimSpace(string(secretBytes))
	return secret, nil
}

// AuthInterceptor creates a gRPC client interceptor that adds authorization headers
func AuthInterceptor(secret string) grpc.UnaryClientInterceptor {
	return func(ctx context.Context, method string, req, reply interface{}, cc *grpc.ClientConn, invoker grpc.UnaryInvoker, opts ...grpc.CallOption) error {
		// Add authorization header to the context
		ctx = metadata.AppendToOutgoingContext(ctx, "authorization", secret)
		return invoker(ctx, method, req, reply, cc, opts...)
	}
}

// StreamAuthInterceptor creates a gRPC client stream interceptor that adds authorization headers
func StreamAuthInterceptor(secret string) grpc.StreamClientInterceptor {
	return func(ctx context.Context, desc *grpc.StreamDesc, cc *grpc.ClientConn, method string, streamer grpc.Streamer, opts ...grpc.CallOption) (grpc.ClientStream, error) {
		// Add authorization header to the context
		ctx = metadata.AppendToOutgoingContext(ctx, "authorization", secret)
		return streamer(ctx, desc, cc, method, opts...)
	}
}

