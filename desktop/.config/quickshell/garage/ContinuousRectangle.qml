import QtQuick
import QtQuick.Shapes

Item {
    id: root

    default property alias contentData: content.data
    property color color: "transparent"
    property color borderColor: "transparent"
    property real borderWidth: 0
    // Insets the rendered outline while the Item itself keeps its full size.
    // Useful for texture masks, where changing the mask item's geometry would
    // make GraphicalEffects rescale it back over the source texture.
    property real outlineInset: 0
    property real radius: Theme.cornerRadius
    property real power: Theme.cornerPower
    readonly property real effectiveRadius: Math.min(
        radius * power / 2, width / 2, height / 2)

    function outlinePath(inset) {
        const x0 = inset;
        const y0 = inset;
        const x1 = root.width - inset;
        const y1 = root.height - inset;
        const r = Math.max(0, Math.min(root.effectiveRadius - inset,
            (x1 - x0) / 2, (y1 - y0) / 2));
        if (r <= 0)
            return `M ${x0} ${y0} L ${x1} ${y0} L ${x1} ${y1} L ${x0} ${y1} Z`;

        // Match Hyprland's visible client/frame curve. CWindow::rounding()
        // supplies the effective radius above; rounding.glsl uses this
        // superellipse with the configured rounding power.
        const exponent = 2 / Math.max(root.power, 1);
        const samples = 24;
        let path = `M ${x0 + r} ${y0} L ${x1 - r} ${y0}`;

        function corner(cx, cy, startAngle) {
            for (let index = 1; index <= samples; ++index) {
                const angle = startAngle + index * Math.PI / (2 * samples);
                const cosine = Math.cos(angle);
                const sine = Math.sin(angle);
                const x = cx + r * Math.sign(cosine)
                    * Math.pow(Math.abs(cosine), exponent);
                const y = cy + r * Math.sign(sine)
                    * Math.pow(Math.abs(sine), exponent);
                path += ` L ${x} ${y}`;
            }
        }

        corner(x1 - r, y0 + r, -Math.PI / 2);
        path += ` L ${x1} ${y1 - r}`;
        corner(x1 - r, y1 - r, 0);
        path += ` L ${x0 + r} ${y1}`;
        corner(x0 + r, y1 - r, Math.PI / 2);
        path += ` L ${x0} ${y0 + r}`;
        corner(x0 + r, y0 + r, Math.PI);
        return path + " Z";
    }

    Shape {
        anchors.fill: parent
        antialiasing: true
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            fillColor: root.color
            strokeColor: root.borderWidth > 0 ? root.borderColor : "transparent"
            strokeWidth: root.borderWidth
            joinStyle: ShapePath.RoundJoin
            PathSvg {
                path: root.outlinePath(Math.max(root.outlineInset,
                    root.borderWidth / 2))
            }
        }
    }

    Item {
        id: content
        anchors.fill: parent
    }
}
