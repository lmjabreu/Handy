import ActivityKit
import Foundation

/// Shared by the app's bridge and the widget extension: ActivityKit matches
/// an activity to its widget by this type, so both must compile this file.
@available(macOS 27.0, *)
struct HandyIslandAttributes: ActivityAttributes {
    struct ContentState: Codable, Hashable {
        /// "recording", "transcribing" or "processing".
        var phase: String
        /// The phase's label in Handy's language, from Handy's translations;
        /// empty while listening.
        var label: String
        /// Live transcript: settled text, then the part the model may still rewrite.
        var committed: String
        var tentative: String
        /// When capture started, so the widget's timer runs without updates.
        var startedAt: Date
    }
}
