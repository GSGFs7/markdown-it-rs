import { isDeepStrictEqual } from "node:util";

// Baselines pin both sides. Fixed, changed, or new mismatches require review.
export function compare(fixture, jsMatches, rustMatches, baseline) {
  if (fixture.expected && !isDeepStrictEqual(jsMatches, fixture.expected)) {
    return "JS oracle changed";
  }
  if (baseline && fixture.knownDifference) {
    if (isDeepStrictEqual(jsMatches, rustMatches)) return "known difference resolved; remove its baseline";
    if (!isDeepStrictEqual(rustMatches, fixture.knownDifference.rust)) return "known Rust difference changed";
    return null;
  }
  return isDeepStrictEqual(jsMatches, rustMatches) ? null : "detection mismatch";
}
