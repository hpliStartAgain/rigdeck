import { readFile, readdir } from "node:fs/promises";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceRoot = join(repository, "apps", "desktop", "src");
const typescriptPath = join(repository, "apps", "desktop", "node_modules", "typescript", "lib", "typescript.js");
const ts = await import(pathToFileURL(typescriptPath).href);
const files = await collect(sourceRoot);
const violations = [];

for (const file of files.filter((value) => [".tsx", ".jsx"].includes(extname(value)))) {
  const source = await readFile(file, "utf8");
  const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  visit(tree, file, tree);
}

if (violations.length > 0) {
  console.error("发现未进入 i18n 的 JSX 可见字面量：");
  for (const violation of violations) console.error(`- ${violation}`);
  process.exitCode = 1;
} else {
  console.log(`i18n JSX 门禁通过：${files.length} 个前端源文件`);
}

function visit(node, file, tree) {
  if (ts.isJsxText(node)) check(node.text, node, file, tree);
  if (ts.isJsxAttribute(node) && node.initializer && ts.isStringLiteral(node.initializer)) {
    const name = node.name.getText(tree);
    if (["alt", "aria-label", "placeholder", "title"].includes(name)) {
      check(node.initializer.text, node.initializer, file, tree);
    }
  }
  if (ts.isJsxExpression(node) && node.expression && ts.isStringLiteral(node.expression)) {
    check(node.expression.text, node.expression, file, tree);
  }
  ts.forEachChild(node, (child) => visit(child, file, tree));
}

function check(raw, node, file, tree) {
  const value = raw.replace(/\s+/g, " ").trim();
  if (!value || value === "RigDeck" || /^[\s·—…:/.+()#-]+$/.test(value)) return;
  const position = tree.getLineAndCharacterOfPosition(node.getStart(tree));
  violations.push(`${file.slice(repository.length + 1)}:${position.line + 1} -> ${JSON.stringify(value)}`);
}

async function collect(root) {
  const output = [];
  for (const entry of await readdir(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) output.push(...await collect(path));
    else if (entry.isFile()) output.push(path);
  }
  return output;
}
