import commonjs from "@rollup/plugin-commonjs";
import { nodeResolve } from "@rollup/plugin-node-resolve";
import typescript from "@rollup/plugin-typescript";

export default [
  {
    input: "src/index.ts",
    output: {
      file: "dist/index.js",
      format: "es",
      sourcemap: true,
      esModule: true,
    },
    plugins: [
      typescript(),
      nodeResolve({
        preferBuiltins: true,
      }),
      commonjs(),
    ],
  },
  {
    input: "src/cleanup.ts",
    output: {
      file: "dist/cleanup.js",
      format: "es",
      sourcemap: true,
      esModule: true,
    },
    plugins: [
      typescript(),
      nodeResolve({
        preferBuiltins: true,
      }),
      commonjs(),
    ],
  },
];
