import loadCrate from "../../../crates/jsona-wasm-core/Cargo.toml";
import { Ast, FormatOptions, ToAstResult, ToJsonResult } from "./types";
export * as types from "./types";
export default class Jsona {
  private static crate: any | undefined;
  private static guard: boolean = false;
  private constructor() {
    if (!Jsona.guard) {
      throw new Error(
        `an instance of Jsona can only be created by calling the "getInstance" static method`
      );
    }
  }

  public static async getInstance(): Promise<Jsona> {
    if (typeof Jsona.crate === "undefined") {
      Jsona.crate = await loadCrate();
    }
    Jsona.guard = true;
    const self = new Jsona();
    Jsona.guard = false;
    return self;
  }

  /**
   * Parse jsona doc as json
   * @param input JSONA document.
   */
  public parse(input: string): ToJsonResult {
    try {
      return { value: Jsona.crate.parse(input) }
    } catch (errors) {
      return { errors: errors }
    }
  }


  /**
   * Parse jsona doc as ast
   * @param input JSONA document.
   */
  public parseAst(input: string): ToAstResult {
    try {
      return { value: Jsona.crate.parse_ast(input) }
    } catch (errors) {
      return { errors: errors }
    }
  }

  /**
   *  Stringify ast to jsona doc
   * @param ast JSONA ast.
   */
  public stringifyAst(ast: Ast): String {
    return Jsona.crate.stringify_ast(ast);
  }

  /**
   * Format the given JSONA document.
   *
   * @param jsona JSONA document.
   * @param options Optional format options.
   */
  public format(jsona: string, options?: FormatOptions): string {
    try {
      return Jsona.crate.format(
        jsona,
        options ?? {},
      );
    } catch (e) {
      throw new Error(e);
    }
  }
}