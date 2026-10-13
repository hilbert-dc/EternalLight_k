export function parseModelCliOptions(args) {
  const result = { connectionId: null, profileId: null, help: false };
  for (let index = 0; index < args.length; index += 1) {
    const value = args[index];
    if (["--help", "-h"].includes(value)) result.help = true;
    else if (value === "--profile") {
      if (result.profileId !== null) throw new Error("--profile may only be specified once");
      const profile = args[++index];
      if (!profile || profile.startsWith("-")) throw new Error("--profile requires a value");
      result.profileId = profile;
    } else if (value.startsWith("-")) throw new Error(`unknown option: ${value}`);
    else {
      if (result.connectionId !== null) throw new Error("only one connection-id is allowed");
      result.connectionId = value;
    }
  }
  return result;
}
