import { spawn } from "node:child_process";
import readline from "node:readline";

export function rustWorker(binary, cwd) {
  const child = spawn(binary, [], { cwd, stdio: ["pipe", "pipe", "inherit"] });
  let pending;
  let failure;
  function fail(error) {
    failure ??= error;
    pending?.reject(error);
    pending = undefined;
    child.kill("SIGKILL");
  }
  child.on("error", fail);
  child.stdin.on("error", fail);
  child.on("exit", (code, signal) => fail(new Error(`Rust adapter exited: ${code ?? signal}`)));
  readline.createInterface({ input: child.stdout }).on("line", (line) => {
    if (!pending) return fail(new Error("Rust adapter returned an unsolicited response"));
    const current = pending;
    pending = undefined;
    try {
      const result = JSON.parse(line);
      if (!Array.isArray(result.matches)) throw new Error(`Rust adapter: ${result.error ?? "invalid response"}`);
      current.resolve(result.matches);
    } catch (error) {
      current.reject(error);
    }
  });
  return {
    request(request) {
      if (failure) return Promise.reject(failure);
      if (pending) return Promise.reject(new Error("Rust adapter already has a pending request"));
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => fail(new Error("Rust adapter exceeded 5000ms")), 5000);
        pending = {
          resolve(value) { clearTimeout(timer); resolve(value); },
          reject(error) { clearTimeout(timer); reject(error); },
        };
        child.stdin.write(JSON.stringify(request) + "\n");
      });
    },
    close() { child.kill("SIGKILL"); },
  };
}
