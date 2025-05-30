// Code generated by protoc-gen-go-grpc. DO NOT EDIT.
// versions:
// - protoc-gen-go-grpc v1.5.1
// - protoc             v4.25.4
// source: context/context.proto

package context

import (
	context "context"
	artifact "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	grpc "google.golang.org/grpc"
	codes "google.golang.org/grpc/codes"
	status "google.golang.org/grpc/status"
)

// This is a compile-time assertion to ensure that this generated file
// is compatible with the grpc package it is being compiled against.
// Requires gRPC-Go v1.64.0 or later.
const _ = grpc.SupportPackageIsVersion9

const (
	ContextService_GetArtifact_FullMethodName  = "/vorpal.context.ContextService/GetArtifact"
	ContextService_GetArtifacts_FullMethodName = "/vorpal.context.ContextService/GetArtifacts"
)

// ContextServiceClient is the client API for ContextService service.
//
// For semantics around ctx use and closing/ending streaming RPCs, please refer to https://pkg.go.dev/google.golang.org/grpc/?tab=doc#ClientConn.NewStream.
type ContextServiceClient interface {
	GetArtifact(ctx context.Context, in *artifact.ArtifactRequest, opts ...grpc.CallOption) (*artifact.Artifact, error)
	GetArtifacts(ctx context.Context, in *artifact.ArtifactsRequest, opts ...grpc.CallOption) (*artifact.ArtifactsResponse, error)
}

type contextServiceClient struct {
	cc grpc.ClientConnInterface
}

func NewContextServiceClient(cc grpc.ClientConnInterface) ContextServiceClient {
	return &contextServiceClient{cc}
}

func (c *contextServiceClient) GetArtifact(ctx context.Context, in *artifact.ArtifactRequest, opts ...grpc.CallOption) (*artifact.Artifact, error) {
	cOpts := append([]grpc.CallOption{grpc.StaticMethod()}, opts...)
	out := new(artifact.Artifact)
	err := c.cc.Invoke(ctx, ContextService_GetArtifact_FullMethodName, in, out, cOpts...)
	if err != nil {
		return nil, err
	}
	return out, nil
}

func (c *contextServiceClient) GetArtifacts(ctx context.Context, in *artifact.ArtifactsRequest, opts ...grpc.CallOption) (*artifact.ArtifactsResponse, error) {
	cOpts := append([]grpc.CallOption{grpc.StaticMethod()}, opts...)
	out := new(artifact.ArtifactsResponse)
	err := c.cc.Invoke(ctx, ContextService_GetArtifacts_FullMethodName, in, out, cOpts...)
	if err != nil {
		return nil, err
	}
	return out, nil
}

// ContextServiceServer is the server API for ContextService service.
// All implementations must embed UnimplementedContextServiceServer
// for forward compatibility.
type ContextServiceServer interface {
	GetArtifact(context.Context, *artifact.ArtifactRequest) (*artifact.Artifact, error)
	GetArtifacts(context.Context, *artifact.ArtifactsRequest) (*artifact.ArtifactsResponse, error)
	mustEmbedUnimplementedContextServiceServer()
}

// UnimplementedContextServiceServer must be embedded to have
// forward compatible implementations.
//
// NOTE: this should be embedded by value instead of pointer to avoid a nil
// pointer dereference when methods are called.
type UnimplementedContextServiceServer struct{}

func (UnimplementedContextServiceServer) GetArtifact(context.Context, *artifact.ArtifactRequest) (*artifact.Artifact, error) {
	return nil, status.Errorf(codes.Unimplemented, "method GetArtifact not implemented")
}
func (UnimplementedContextServiceServer) GetArtifacts(context.Context, *artifact.ArtifactsRequest) (*artifact.ArtifactsResponse, error) {
	return nil, status.Errorf(codes.Unimplemented, "method GetArtifacts not implemented")
}
func (UnimplementedContextServiceServer) mustEmbedUnimplementedContextServiceServer() {}
func (UnimplementedContextServiceServer) testEmbeddedByValue()                        {}

// UnsafeContextServiceServer may be embedded to opt out of forward compatibility for this service.
// Use of this interface is not recommended, as added methods to ContextServiceServer will
// result in compilation errors.
type UnsafeContextServiceServer interface {
	mustEmbedUnimplementedContextServiceServer()
}

func RegisterContextServiceServer(s grpc.ServiceRegistrar, srv ContextServiceServer) {
	// If the following call panics, it indicates UnimplementedContextServiceServer was
	// embedded by pointer and is nil.  This will cause panics if an
	// unimplemented method is ever invoked, so we test this at initialization
	// time to prevent it from happening at runtime later due to I/O.
	if t, ok := srv.(interface{ testEmbeddedByValue() }); ok {
		t.testEmbeddedByValue()
	}
	s.RegisterService(&ContextService_ServiceDesc, srv)
}

func _ContextService_GetArtifact_Handler(srv interface{}, ctx context.Context, dec func(interface{}) error, interceptor grpc.UnaryServerInterceptor) (interface{}, error) {
	in := new(artifact.ArtifactRequest)
	if err := dec(in); err != nil {
		return nil, err
	}
	if interceptor == nil {
		return srv.(ContextServiceServer).GetArtifact(ctx, in)
	}
	info := &grpc.UnaryServerInfo{
		Server:     srv,
		FullMethod: ContextService_GetArtifact_FullMethodName,
	}
	handler := func(ctx context.Context, req interface{}) (interface{}, error) {
		return srv.(ContextServiceServer).GetArtifact(ctx, req.(*artifact.ArtifactRequest))
	}
	return interceptor(ctx, in, info, handler)
}

func _ContextService_GetArtifacts_Handler(srv interface{}, ctx context.Context, dec func(interface{}) error, interceptor grpc.UnaryServerInterceptor) (interface{}, error) {
	in := new(artifact.ArtifactsRequest)
	if err := dec(in); err != nil {
		return nil, err
	}
	if interceptor == nil {
		return srv.(ContextServiceServer).GetArtifacts(ctx, in)
	}
	info := &grpc.UnaryServerInfo{
		Server:     srv,
		FullMethod: ContextService_GetArtifacts_FullMethodName,
	}
	handler := func(ctx context.Context, req interface{}) (interface{}, error) {
		return srv.(ContextServiceServer).GetArtifacts(ctx, req.(*artifact.ArtifactsRequest))
	}
	return interceptor(ctx, in, info, handler)
}

// ContextService_ServiceDesc is the grpc.ServiceDesc for ContextService service.
// It's only intended for direct use with grpc.RegisterService,
// and not to be introspected or modified (even as a copy)
var ContextService_ServiceDesc = grpc.ServiceDesc{
	ServiceName: "vorpal.context.ContextService",
	HandlerType: (*ContextServiceServer)(nil),
	Methods: []grpc.MethodDesc{
		{
			MethodName: "GetArtifact",
			Handler:    _ContextService_GetArtifact_Handler,
		},
		{
			MethodName: "GetArtifacts",
			Handler:    _ContextService_GetArtifacts_Handler,
		},
	},
	Streams:  []grpc.StreamDesc{},
	Metadata: "context/context.proto",
}
