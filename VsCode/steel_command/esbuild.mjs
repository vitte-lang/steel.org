import esbuild from "esbuild";

const watch = process.argv.includes("--watch");

const shared = {
  bundle: true,
  platform: "node",
  format: "cjs",
  target: "node18",
  sourcemap: false,
  logLevel: "info",
};

const builds = [
  {
    entryPoints: ["src/extension.ts"],
    outfile: "dist/extension.js",
    external: ["vscode"],
  },
  {
    entryPoints: ["server/src/server.ts"],
    outfile: "server/dist/server.js",
  },
];

async function run() {
  if (watch) {
    const contexts = await Promise.all(
      builds.map((build) => esbuild.context({ ...shared, ...build }))
    );
    await Promise.all(contexts.map((context) => context.watch()));
    return;
  }

  await Promise.all(builds.map((build) => esbuild.build({ ...shared, ...build })));
}

run().catch((err) => {
  console.error(err);
  process.exit(1);
});
