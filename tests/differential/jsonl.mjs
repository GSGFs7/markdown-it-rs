// JSON strings may contain literal U+2028/U+2029. Frame only on LF, rather than
// relying on readline's definition of a line separator.
export async function* lines(stream) {
  stream.setEncoding("utf8");
  let pending = "";
  for await (const chunk of stream) {
    pending += chunk;
    let end;
    while ((end = pending.indexOf("\n")) !== -1) {
      yield pending.slice(0, end);
      pending = pending.slice(end + 1);
    }
  }
  if (pending) throw new Error("incomplete JSONL response");
}
