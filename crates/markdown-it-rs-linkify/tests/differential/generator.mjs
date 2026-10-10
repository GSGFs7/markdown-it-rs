// Deterministic linkify mutations, owned by this crate's test corpus.
export function random(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let value = Math.imul(state ^ (state >>> 15), 1 | state);
    value ^= value + Math.imul(value ^ (value >>> 7), 61 | value);
    return ((value ^ (value >>> 14)) >>> 0) / 4294967296;
  };
}

const alphabet = Array.from("[](){}!<>\\@.:/?#|`_-; \n\r\t abc09中文é😀\u0000");
export function mutate(source, rng) {
  const chars = Array.from(source).slice(0, 4096);
  const index = Math.floor(rng() * (chars.length + 1));
  const count = 1 + Math.floor(rng() * 8);
  const inserted = Array.from({ length: count }, () => alphabet[Math.floor(rng() * alphabet.length)]);
  switch (Math.floor(rng() * 5)) {
    case 0: chars.splice(index, count); break;
    case 1: chars.splice(index, 0, ...inserted); break;
    case 2: chars.splice(index, count, ...inserted); break;
    case 3: chars.splice(index, 0, ...chars.slice(index, index + count)); break;
    case 4: chars.length = index; break;
  }
  return chars.slice(0, 4096).join("");
}
