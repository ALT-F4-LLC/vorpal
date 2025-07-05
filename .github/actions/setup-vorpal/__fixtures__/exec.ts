import * as exec from "@actions/exec";
import { jest } from "@jest/globals";

export const exec_fn = jest.fn<typeof exec.exec>();
