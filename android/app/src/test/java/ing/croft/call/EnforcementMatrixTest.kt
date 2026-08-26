package ing.croft.call

import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * The client half of the enforcement scenario matrix stays honest
 * (plans/2026-08-26-1-plan-enforcement-scenario-matrix.md; the server half
 * lives in croft-stack docs/ENFORCEMENT-SCENARIOS.md with its own bats gate).
 *
 * docs/ENFORCEMENT-SCENARIOS.md here names, for every client-posture row —
 * what must dial, what must camp, what must degrade tokenless WITH words,
 * what the screen must say — the test that pins it, as
 * PIN:File.kt::`test name`. This gate fails when a named test does not exist
 * and when a row is an unresolved GAP: an unwired scenario silently reads as
 * covered, which is the failure mode the matrix exists to kill (its own v1
 * gap analysis produced two false gaps by trusting memory over files).
 *
 * Runs inside testDebugUnitTest, so `make gate` and CI carry it without new
 * plumbing.
 */
class EnforcementMatrixTest {

    private fun repoRoot(): File {
        var dir = File("").absoluteFile
        while (!File(dir, ".git").exists()) {
            dir = dir.parentFile ?: error("no repo root above ${File("").absolutePath}")
        }
        return dir
    }

    private fun doc(): File = File(repoRoot(), "docs/ENFORCEMENT-SCENARIOS.md")

    @Test
    fun `the client matrix exists and carries all three outcome classes`() {
        val d = doc()
        assertTrue("missing ${d.path}", d.isFile)
        val text = d.readText()
        for (cls in listOf("MUST DIAL", "MUST REFUSE", "MUST DEGRADE", "MUST SAY")) {
            assertTrue("matrix carries no $cls rows", text.contains(cls))
        }
    }

    @Test
    fun `every PIN names a test that exists under the android test tree`() {
        val d = doc()
        assertTrue("missing ${d.path}", d.isFile)
        val testRoot = File(repoRoot(), "android/app/src/test")
        val pins = Regex("PIN:([A-Za-z0-9]+\\.kt)::`([^`]+)`").findAll(d.readText()).toList()
        assertTrue("the matrix carries no PINs at all", pins.size >= 15)
        val failures = pins.mapNotNull { m ->
            val (fileName, testName) = m.destructured
            val file = testRoot.walkTopDown().firstOrNull { it.name == fileName }
                ?: return@mapNotNull "MISSING FILE $fileName (for `$testName`)"
            if (!file.readText().contains("fun `$testName`")) {
                "MISSING TEST `$testName` in ${file.name}"
            } else null
        }
        assertTrue(failures.joinToString("\n"), failures.isEmpty())
    }

    @Test
    fun `no unresolved GAP rows`() {
        val d = doc()
        assertTrue("missing ${d.path}", d.isFile)
        // Only table ROWS can carry a GAP marker; prose may name the rule.
        val gaps = d.readLines().filter { it.trimStart().startsWith("|") && it.contains("GAP") }
        assertTrue("unresolved GAP rows:\n${gaps.joinToString("\n")}", gaps.isEmpty())
    }
}
