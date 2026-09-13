import unittest

from runtime.creative_workspace.context_limits import fit_context_snapshot
from runtime.creative_workspace.errors import WorkspaceError


class ContextLimitTests(unittest.TestCase):
    def test_optional_material_is_removed_before_author_requirements(self) -> None:
        commitment = {"publicCommitments": ["父亲身份不得在本卷揭露"]}
        forbidden = {"policy": "当前视角不得知道密室钥匙属于谁"}
        snapshot = {
            "sourceExcerpts": [{"body": "摘录" * 3_000}],
            "facts": [{"evidence": "事实" * 1_000}],
            "commitments": [commitment],
            "forbidden": [forbidden],
            "hardConstraints": [{"statement": "保持第一人称"}],
            "omissions": {},
        }

        fitted = fit_context_snapshot(snapshot, 4_000)

        self.assertEqual(fitted["commitments"], [commitment])
        self.assertEqual(fitted["forbidden"], [forbidden])
        self.assertEqual(fitted["sourceExcerpts"], [])
        self.assertTrue(fitted["contextBudget"]["truncated"])

    def test_oversized_author_requirements_fail_closed(self) -> None:
        snapshot = {
            "sourceExcerpts": [],
            "facts": [],
            "commitments": [{"publicCommitments": ["不可裁剪" * 2_000]}],
            "forbidden": [],
            "hardConstraints": [],
            "omissions": {},
        }

        with self.assertRaisesRegex(WorkspaceError, "上下文预算不足"):
            fit_context_snapshot(snapshot, 4_000)


if __name__ == "__main__":
    unittest.main()
