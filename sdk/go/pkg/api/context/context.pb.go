// Code generated by protoc-gen-go. DO NOT EDIT.
// versions:
// 	protoc-gen-go v1.36.3
// 	protoc        v4.25.4
// source: context/context.proto

package context

import (
	artifact "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	protoreflect "google.golang.org/protobuf/reflect/protoreflect"
	protoimpl "google.golang.org/protobuf/runtime/protoimpl"
	reflect "reflect"
)

const (
	// Verify that this generated code is sufficiently up-to-date.
	_ = protoimpl.EnforceVersion(20 - protoimpl.MinVersion)
	// Verify that runtime/protoimpl is sufficiently up-to-date.
	_ = protoimpl.EnforceVersion(protoimpl.MaxVersion - 20)
)

var File_context_context_proto protoreflect.FileDescriptor

var file_context_context_proto_rawDesc = []byte{
	0x0a, 0x15, 0x63, 0x6f, 0x6e, 0x74, 0x65, 0x78, 0x74, 0x2f, 0x63, 0x6f, 0x6e, 0x74, 0x65, 0x78,
	0x74, 0x2e, 0x70, 0x72, 0x6f, 0x74, 0x6f, 0x12, 0x0e, 0x76, 0x6f, 0x72, 0x70, 0x61, 0x6c, 0x2e,
	0x63, 0x6f, 0x6e, 0x74, 0x65, 0x78, 0x74, 0x1a, 0x17, 0x61, 0x72, 0x74, 0x69, 0x66, 0x61, 0x63,
	0x74, 0x2f, 0x61, 0x72, 0x74, 0x69, 0x66, 0x61, 0x63, 0x74, 0x2e, 0x70, 0x72, 0x6f, 0x74, 0x6f,
	0x32, 0xb3, 0x01, 0x0a, 0x0e, 0x43, 0x6f, 0x6e, 0x74, 0x65, 0x78, 0x74, 0x53, 0x65, 0x72, 0x76,
	0x69, 0x63, 0x65, 0x12, 0x4a, 0x0a, 0x0b, 0x47, 0x65, 0x74, 0x41, 0x72, 0x74, 0x69, 0x66, 0x61,
	0x63, 0x74, 0x12, 0x20, 0x2e, 0x76, 0x6f, 0x72, 0x70, 0x61, 0x6c, 0x2e, 0x61, 0x72, 0x74, 0x69,
	0x66, 0x61, 0x63, 0x74, 0x2e, 0x41, 0x72, 0x74, 0x69, 0x66, 0x61, 0x63, 0x74, 0x52, 0x65, 0x71,
	0x75, 0x65, 0x73, 0x74, 0x1a, 0x19, 0x2e, 0x76, 0x6f, 0x72, 0x70, 0x61, 0x6c, 0x2e, 0x61, 0x72,
	0x74, 0x69, 0x66, 0x61, 0x63, 0x74, 0x2e, 0x41, 0x72, 0x74, 0x69, 0x66, 0x61, 0x63, 0x74, 0x12,
	0x55, 0x0a, 0x0c, 0x47, 0x65, 0x74, 0x41, 0x72, 0x74, 0x69, 0x66, 0x61, 0x63, 0x74, 0x73, 0x12,
	0x21, 0x2e, 0x76, 0x6f, 0x72, 0x70, 0x61, 0x6c, 0x2e, 0x61, 0x72, 0x74, 0x69, 0x66, 0x61, 0x63,
	0x74, 0x2e, 0x41, 0x72, 0x74, 0x69, 0x66, 0x61, 0x63, 0x74, 0x73, 0x52, 0x65, 0x71, 0x75, 0x65,
	0x73, 0x74, 0x1a, 0x22, 0x2e, 0x76, 0x6f, 0x72, 0x70, 0x61, 0x6c, 0x2e, 0x61, 0x72, 0x74, 0x69,
	0x66, 0x61, 0x63, 0x74, 0x2e, 0x41, 0x72, 0x74, 0x69, 0x66, 0x61, 0x63, 0x74, 0x73, 0x52, 0x65,
	0x73, 0x70, 0x6f, 0x6e, 0x73, 0x65, 0x42, 0x35, 0x5a, 0x33, 0x67, 0x69, 0x74, 0x68, 0x75, 0x62,
	0x2e, 0x63, 0x6f, 0x6d, 0x2f, 0x41, 0x4c, 0x54, 0x2d, 0x46, 0x34, 0x2d, 0x4c, 0x4c, 0x43, 0x2f,
	0x76, 0x6f, 0x72, 0x70, 0x61, 0x6c, 0x2f, 0x73, 0x64, 0x6b, 0x2f, 0x67, 0x6f, 0x2f, 0x70, 0x6b,
	0x67, 0x2f, 0x61, 0x70, 0x69, 0x2f, 0x63, 0x6f, 0x6e, 0x74, 0x65, 0x78, 0x74, 0x62, 0x06, 0x70,
	0x72, 0x6f, 0x74, 0x6f, 0x33,
}

var file_context_context_proto_goTypes = []any{
	(*artifact.ArtifactRequest)(nil),   // 0: vorpal.artifact.ArtifactRequest
	(*artifact.ArtifactsRequest)(nil),  // 1: vorpal.artifact.ArtifactsRequest
	(*artifact.Artifact)(nil),          // 2: vorpal.artifact.Artifact
	(*artifact.ArtifactsResponse)(nil), // 3: vorpal.artifact.ArtifactsResponse
}
var file_context_context_proto_depIdxs = []int32{
	0, // 0: vorpal.context.ContextService.GetArtifact:input_type -> vorpal.artifact.ArtifactRequest
	1, // 1: vorpal.context.ContextService.GetArtifacts:input_type -> vorpal.artifact.ArtifactsRequest
	2, // 2: vorpal.context.ContextService.GetArtifact:output_type -> vorpal.artifact.Artifact
	3, // 3: vorpal.context.ContextService.GetArtifacts:output_type -> vorpal.artifact.ArtifactsResponse
	2, // [2:4] is the sub-list for method output_type
	0, // [0:2] is the sub-list for method input_type
	0, // [0:0] is the sub-list for extension type_name
	0, // [0:0] is the sub-list for extension extendee
	0, // [0:0] is the sub-list for field type_name
}

func init() { file_context_context_proto_init() }
func file_context_context_proto_init() {
	if File_context_context_proto != nil {
		return
	}
	type x struct{}
	out := protoimpl.TypeBuilder{
		File: protoimpl.DescBuilder{
			GoPackagePath: reflect.TypeOf(x{}).PkgPath(),
			RawDescriptor: file_context_context_proto_rawDesc,
			NumEnums:      0,
			NumMessages:   0,
			NumExtensions: 0,
			NumServices:   1,
		},
		GoTypes:           file_context_context_proto_goTypes,
		DependencyIndexes: file_context_context_proto_depIdxs,
	}.Build()
	File_context_context_proto = out.File
	file_context_context_proto_rawDesc = nil
	file_context_context_proto_goTypes = nil
	file_context_context_proto_depIdxs = nil
}
