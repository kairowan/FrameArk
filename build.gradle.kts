import groovy.json.JsonOutput
import org.gradle.api.artifacts.component.ModuleComponentIdentifier
import java.security.MessageDigest

plugins {
    id("com.android.application") version "8.13.2" apply false
    id("org.jetbrains.kotlin.android") version "2.2.20" apply false
}

// Resolve the actual release runtime, not a cache scan or a hand-maintained list.
// Private report paths are consumed locally; the public notice index omits them.
subprojects {
    plugins.withId("com.android.application") {
        tasks.register("writeReleaseRuntimeInventory") {
            val report = layout.buildDirectory.file("reports/frameark/release-runtime.json")
            outputs.file(report)
            outputs.upToDateWhen { false }
            doLast {
                val artifacts = configurations.getByName("releaseRuntimeClasspath")
                    .incoming.artifacts.artifacts.map { artifact ->
                        val id = artifact.id.componentIdentifier as? ModuleComponentIdentifier
                            ?: error("Unreviewed non-module Android runtime dependency")
                        val hash = MessageDigest.getInstance("SHA-256")
                        artifact.file.inputStream().use { stream ->
                            val buffer = ByteArray(65536)
                            while (true) {
                                val count = stream.read(buffer)
                                if (count < 0) break
                                hash.update(buffer, 0, count)
                            }
                        }
                        mapOf(
                            "coordinate" to "${id.group}:${id.module}:${id.version}",
                            "file" to artifact.file.absolutePath,
                            "sha256" to hash.digest().joinToString("") { "%02x".format(it) },
                        )
                    }.sortedBy { it.getValue("coordinate") }
                val output = report.get().asFile
                output.parentFile.mkdirs()
                output.writeText(JsonOutput.prettyPrint(JsonOutput.toJson(mapOf(
                    "schema" to 1, "app" to project.name,
                    "configuration" to "releaseRuntimeClasspath", "artifacts" to artifacts,
                ))) + "\n", Charsets.UTF_8)
            }
        }
    }
}

tasks.register("writeReleaseRuntimeInventories") {
    dependsOn(":receiver:writeReleaseRuntimeInventory", ":sender:writeReleaseRuntimeInventory")
}
