import SwiftUI

struct ToolGroupsLayout: Layout {
    var rows: Bool
    var spacing: CGFloat
    private func geometry(width: CGFloat, subviews: Subviews) -> (columns: [CGFloat], heights: [CGFloat]) {
        let columns = ToolbarUI.cached(["type": "tool_set_columns", "width": width,
            "natural": subviews.map { ceil($0.sizeThatFits(.unspecified).width) }, "rows": rows]).array.map { CGFloat($0.number) }
        guard !columns.isEmpty else { return ([], []) }
        var heights = Array(repeating: CGFloat.zero, count: (subviews.count + columns.count - 1) / columns.count)
        for index in subviews.indices {
            heights[index / columns.count] = max(heights[index / columns.count],
                subviews[index].sizeThatFits(ProposedViewSize(width: columns[index % columns.count], height: nil)).height)
        }
        return (columns, heights)
    }
    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width.flatMap { $0.isFinite ? $0 : nil } ?? subviews.map { $0.sizeThatFits(.unspecified).width }.max() ?? 0
        let (_, heights) = geometry(width: width, subviews: subviews)
        return CGSize(width: width, height: heights.reduce(0, +) + spacing * CGFloat(max(0, heights.count - 1)))
    }
    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let (columns, heights) = geometry(width: bounds.width, subviews: subviews)
        guard !columns.isEmpty else { return }
        var x = bounds.minX, y = bounds.minY
        for index in subviews.indices {
            let column = index % columns.count, row = index / columns.count
            if column == 0 { x = bounds.minX; if row > 0 { y += heights[row - 1] + spacing } }
            subviews[index].place(at: CGPoint(x: x, y: y), anchor: .topLeading,
                proposal: ProposedViewSize(width: columns[column], height: heights[row]))
            x += columns[column] + spacing
        }
    }
}

struct ToolPreviewLayout: Layout {
    var inset: CGFloat
    private func geometry(width: CGFloat, subviews: Subviews) -> JSON {
        let inner = max(0, width - 2 * inset)
        let caption = subviews.count > 1 ? subviews[1].sizeThatFits(ProposedViewSize(width: inner, height: nil)).height : 0
        return ToolbarUI.cached(["type": "tool_preview", "width": inner, "caption": caption])
    }
    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width.flatMap { $0.isFinite ? $0 : nil } ?? subviews.map { $0.sizeThatFits(.unspecified).width }.max() ?? 0
        return CGSize(width: width, height: geometry(width: width, subviews: subviews)["heights"][1].number)
    }
    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let rectangles = geometry(width: bounds.width, subviews: subviews)["bounds"].array
        for index in subviews.indices where index < rectangles.count {
            let rect = rectangles[index].rect
            subviews[index].place(at: CGPoint(x: bounds.minX + inset + rect.minX, y: bounds.minY + rect.minY),
                anchor: .topLeading, proposal: ProposedViewSize(rect.size))
        }
    }
}
