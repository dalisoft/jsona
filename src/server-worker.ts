import {
  BrowserMessageReader,
  BrowserMessageWriter,
} from "vscode-languageserver-protocol/browser";

import { JsonaLsp, RpcMessage } from "@jsona/lsp";

const worker: Worker = self as any;

const writer = new BrowserMessageWriter(worker);
const reader = new BrowserMessageReader(worker);

let jsona: JsonaLsp;

reader.listen(async message => {
  if (!jsona) {
    jsona = await JsonaLsp.init(
      {
        cwd: () => "/",
        envVar: (name) => {
          if (name === "RUST_LOG") {
            return import.meta.env.RUST_LOG;
          }
          return "";
        },
        findConfigFile: async (_from) => {
          return undefined
        },
        glob: () => [],
        isAbsolute: () => true,
        now: () => new Date(),
        readFile: () => Promise.reject("not implemented"),
        writeFile: () => Promise.reject("not implemented"),
        stderr: async (bytes: Uint8Array) => {
          console.log(new TextDecoder().decode(bytes));
          return bytes.length;
        },
        stdErrAtty: () => false,
        stdin: () => Promise.reject("not implemented"),
        stdout: async (bytes: Uint8Array) => {
          console.log(new TextDecoder().decode(bytes));
          return bytes.length;
        },
        fetchFile: async (url) => {
          const controller = new AbortController();
          const timeout = setTimeout(() => {
            controller.abort();
          }, 10000);
          try {
            const res = await fetch(url, { signal: controller.signal });
            const buf = await res.arrayBuffer();
            return new Uint8Array(buf)
          } catch (err) {
            throw err;
          } finally {
            clearTimeout(timeout);
          }
        },
        urlToFilePath: (url: string) => url.slice("file://".length),
      },
      {
        onMessage(message) {
          debugLog('lsp2host', message);
          writer.write(message);
        },
      }
    );
  }

  debugLog('host2lsp', message);
  jsona.send(message as RpcMessage);
});

function debugLog(topic: string, message: any) {
  if (import.meta.env.RUST_LOG === "debug" || import.meta.env.RUST_LOG == "verbose") {
    console.log(topic, message);
  }
}