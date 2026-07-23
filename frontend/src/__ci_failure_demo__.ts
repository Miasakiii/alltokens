// Temporary file to demonstrate that CI blocks a failing PR from merging.
// It intentionally contains a TypeScript type error (TS2322) and will be reverted.
export const ciFailureDemo: number = "not a number";
