import { Command } from "commander";

export const versionCommand = new Command("version")
  .description("Show version information")
  .action(() => {
    console.log("Solana Validator Switch CLI v1.0.0");
    console.log("Professional-grade validator switching tool");
    console.log("Node.js version:", process.version);
    console.log("Platform:", process.platform);
    console.log("Architecture:", process.arch);
  });
