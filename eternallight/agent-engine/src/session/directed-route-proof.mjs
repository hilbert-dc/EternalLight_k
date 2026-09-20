import { digest } from "../contracts/capabilities.mjs";
import { connectionDefinitionDigest, credentialRealmId } from "../connections/connection-spec.mjs";
import { getCodec } from "../codecs/index.mjs";

export function directedRouteDependencyDigest({ parentConnection, childConnection, parentProtocol, childProtocol, parentCredentialScope = parentConnection.credentialRef, childCredentialScope = childConnection.credentialRef }) {
  return digest({
    edge: { parentConnectionId: parentConnection.id, childConnectionId: childConnection.id },
    parentProtocol,
    childProtocol,
    childConnectionDigest: connectionDefinitionDigest(childConnection),
    childCodecVersion: getCodec(childProtocol).version,
    parentCredentialRealm: credentialRealmId(parentCredentialScope),
    childCredentialRealm: credentialRealmId(childCredentialScope),
  });
}
