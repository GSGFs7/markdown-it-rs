import MarkdownIt from "markdown-it";
import { lines } from "./jsonl.mjs";

for await (const line of lines(process.stdin)) {
  try {
    const { source, preset, options } = JSON.parse(line);
    const html = new MarkdownIt(preset, options).render(source);
    process.stdout.write(JSON.stringify({ html }) + "\n");
  } catch (error) {
    process.stdout.write(JSON.stringify({ error: String(error) }) + "\n");
  }
}
