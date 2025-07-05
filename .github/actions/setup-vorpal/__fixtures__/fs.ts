import * as fs from "fs";
import { jest } from "@jest/globals";

export const openSync = jest.fn<typeof fs.openSync>();
export const closeSync = jest.fn<typeof fs.closeSync>();
export const existsSync = jest.fn<typeof fs.existsSync>();
export const readFileSync = jest.fn<typeof fs.readFileSync>();
