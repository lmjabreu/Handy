import ActivityKit
import SwiftUI
import WidgetKit

// Handy's recording overlay, drawn in the Dynamic Island. Sizes and colours
// follow src/overlay/RecordingOverlay.css so the two read as one design.

/// --dark-color-logo-primary in theme.css.
private let accent = Color(red: 0xF2 / 255, green: 0x8C / 255, blue: 0xBB / 255)
private let muted = Color.white.opacity(0.62)  // --s-muted on the island
private let faint = Color.white.opacity(0.45)  // --s-faint on the island

@main
struct HandyIslandWidgets: WidgetBundle {
    var body: some Widget { HandyIslandActivity() }
}

struct HandyIslandActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: HandyIslandAttributes.self) { context in
            // Menu bar presentation on Macs without an island.
            VStack(alignment: .leading, spacing: 8) {
                ControlRow(state: context.state)
                Transcript(state: context.state).lineLimit(3)
            }
            .padding(14)
            .activityBackgroundTint(.black)
            .activitySystemActionForegroundColor(.white)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    Indicator(state: context.state)
                        .frame(maxHeight: .infinity, alignment: .center)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    Elapsed(since: context.state.startedAt)
                        .frame(maxHeight: .infinity, alignment: .center)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    // Handy sends only the newest words (island.rs), and older
                    // lines give way at the top, as in the overlay. A click on
                    // the text opens the overlay's Live panel, which scrolls.
                    Link(destination: URL(string: "handy://island/transcript")!) {
                        Transcript(state: context.state)
                            .lineLimit(2)
                            // Without this a widget's Text stays on one line and
                            // cuts the newest words off with "…".
                            .fixedSize(horizontal: false, vertical: true)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .padding(.top, 6)
                    .padding(.bottom, 4)
                }
            } compactLeading: {
                PhaseGlyph(phase: context.state.phase)
            } compactTrailing: {
                // While Handy records, macOS's microphone indicator takes the
                // island's other slot and Handy shows as `minimal`, so nothing
                // here or beside the glyph is seen during dictation.
                EmptyView()
            } minimal: {
                PhaseGlyph(phase: context.state.phase)
            }
            // Room from the island's rounded edge, like the overlay card's 14px.
            .contentMargins(.horizontal, 18, for: .expanded)
            .contentMargins(.top, 10, for: .expanded)
            .contentMargins(.bottom, 14, for: .expanded)
        }
    }
}

/// Handy's listening phases: Minimal records as "recording", Live as "streaming".
private func isListening(_ phase: String) -> Bool {
    phase == "recording" || phase == "streaming"
}

/// The overlay's left side: the dot and waveform while listening, a spinner
/// and label while working.
private struct Indicator: View {
    let state: HandyIslandAttributes.ContentState
    var body: some View {
        if isListening(state.phase) {
            HStack(spacing: 10) {
                Dot()
                Waveform()
            }
        } else {
            HStack(spacing: 8) {
                Spinner()
                Text(state.label)
                    .font(.system(size: 12))
                    .foregroundStyle(muted)
            }
        }
    }
}

private struct ControlRow: View {
    let state: HandyIslandAttributes.ContentState
    var body: some View {
        HStack {
            Indicator(state: state)
            Spacer(minLength: 8)
            Elapsed(since: state.startedAt)
        }
    }
}

private struct PhaseGlyph: View {
    let phase: String
    var body: some View {
        if isListening(phase) { Dot() } else { Spinner() }
    }
}

/// .sdot: 7px accent dot with its soft halo.
private struct Dot: View {
    var body: some View {
        Circle()
            .fill(accent)
            .frame(width: 7, height: 7)
            .background(Circle().fill(accent.opacity(0.2)).frame(width: 14, height: 14))
    }
}

/// .swave at rest: nine 4px accent bars, 3px apart. The island can't redraw
/// fast enough to follow the microphone, so it holds a steady shape.
private struct Waveform: View {
    private let heights: [CGFloat] = [6, 10, 14, 9, 16, 12, 8, 13, 7]
    var body: some View {
        HStack(alignment: .center, spacing: 3) {
            ForEach(heights.indices, id: \.self) { i in
                RoundedRectangle(cornerRadius: 2)
                    .fill(accent)
                    .frame(width: 4, height: heights[i])
            }
        }
        .frame(height: 18)
    }
}

/// .sspinner: a 13px ring with an accent arc.
private struct Spinner: View {
    var body: some View {
        ProgressView()
            .progressViewStyle(.circular)
            .controlSize(.mini)
            .tint(accent)
            .frame(width: 13, height: 13)
    }
}

/// .stimer: 12px, faint, tabular digits, counting up without updates.
private struct Elapsed: View {
    let since: Date
    var body: some View {
        Text(timerInterval: since...Date.distantFuture, countsDown: false)
            .font(.system(size: 12).monospacedDigit())
            .foregroundStyle(faint)
            .multilineTextAlignment(.trailing)
            .frame(minWidth: 30)
    }
}

/// .stext-cap: 15px italic; the part the model may still rewrite is dimmed.
private struct Transcript: View {
    let state: HandyIslandAttributes.ContentState
    var body: some View {
        if state.committed.isEmpty && state.tentative.isEmpty {
            // Like the overlay: nothing until there is text to show.
            EmptyView()
        } else {
            let separator = state.committed.isEmpty ? "" : " "
            Text("\(state.committed)\(separator)\(Text(state.tentative).foregroundStyle(muted))")
                .font(.system(size: 15).italic())
                .foregroundStyle(.white)
                .lineSpacing(2)
        }
    }
}
