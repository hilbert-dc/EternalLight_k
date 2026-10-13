import { verifyPackageReport } from "./build-artifacts.mjs";
import { resolveBuildLayout } from "./build-layout.mjs";

const report = verifyPackageReport(resolveBuildLayout().output);
process.stdout.write(`release/ 与 ${report.completedAt} 的打包验证报告一致\n`);
