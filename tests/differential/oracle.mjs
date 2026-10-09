import readline from "node:readline";
import MarkdownIt from "markdown-it";

for await (const line of readline.createInterface({ input: process.stdin })) {
  try {
    const { source, preset, options } = JSON.parse(line);
    const html = new MarkdownIt(preset, options).render(source);
    process.stdout.write(JSON.stringify({ html }) + "\n");
  } catch (error) {
    process.stdout.write(JSON.stringify({ error: String(error) }) + "\n");
  }
}
