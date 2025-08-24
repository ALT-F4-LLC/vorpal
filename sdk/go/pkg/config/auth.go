package config

import (
	"context"
	"fmt"
	"os"
	"strings"

	"google.golang.org/grpc"
	"google.golang.org/grpc/metadata"
)

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

// LoadClientAPIToken loads the user API token from VORPAL_API_TOKEN environment variable
func LoadClientAPIToken() (string, error) {
	if token := os.Getenv("VORPAL_API_TOKEN"); strings.TrimSpace(token) != "" {
		return strings.TrimSpace(token), nil
	}
	return "", fmt.Errorf("VORPAL_API_TOKEN environment variable not set or empty")
}
