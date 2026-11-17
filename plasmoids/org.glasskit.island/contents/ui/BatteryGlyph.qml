import QtQuick
import QtQuick.Shapes

// Horizontal battery drawn from shapes, so it points the same way under every icon theme.
// States: charging = green bolt, battery saver = yellow leaf, low (15 % or less) = red "!",
// otherwise an inner fill that shows the charge level.
Item {
    id: g
    property real percent: 0
    property bool charging: false
    property bool saver: false
    property color normalColor: "#ffffff"

    readonly property bool low: !charging && percent <= 15
    readonly property string mode: charging ? "charging" : saver ? "saver" : low ? "low" : "normal"
    readonly property color c: charging ? "#34d399" : saver ? "#facc15" : low ? "#f87171" : normalColor
    readonly property real bw: Math.max(1.5, height * 0.12)

    implicitHeight: 12
    implicitWidth: implicitHeight * 1.9

    Rectangle {   // body outline
        id: body
        width: g.width - g.height * 0.2; height: g.height
        radius: height * 0.3
        color: "transparent"; border.width: g.bw; border.color: g.c
    }
    Rectangle {   // terminal, on the right
        x: body.width + g.height * 0.05
        anchors.verticalCenter: parent.verticalCenter
        width: g.height * 0.14; height: g.height * 0.4; radius: width / 2
        color: g.c
    }
    Rectangle {   // level fill (normal state only; the other states show a symbol instead)
        visible: g.mode === "normal"
        x: g.bw * 1.8; y: g.bw * 1.8
        height: g.height - g.bw * 3.6; radius: height * 0.3
        width: Math.max(height * 0.5, (body.width - g.bw * 3.6) * Math.min(1, g.percent / 100))
        color: g.c
    }

    Item {   // symbol, centred in the body
        id: sym
        anchors.centerIn: body
        height: body.height * 0.72; width: height

        Shape {   // lightning bolt
            anchors.fill: parent
            visible: g.mode === "charging"
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                fillColor: g.c; strokeColor: "transparent"
                PathPolyline {
                    path: [Qt.point(sym.width * 0.60, 0), Qt.point(sym.width * 0.16, sym.height * 0.58),
                           Qt.point(sym.width * 0.48, sym.height * 0.58), Qt.point(sym.width * 0.40, sym.height),
                           Qt.point(sym.width * 0.84, sym.height * 0.42), Qt.point(sym.width * 0.52, sym.height * 0.42),
                           Qt.point(sym.width * 0.60, 0)]
                }
            }
        }
        Rectangle {   // "!" stem
            visible: g.mode === "low"
            anchors.horizontalCenter: parent.horizontalCenter
            width: sym.width * 0.2; height: sym.height * 0.6; radius: width / 2
            color: g.c
        }
        Rectangle {   // "!" dot
            visible: g.mode === "low"
            anchors.horizontalCenter: parent.horizontalCenter
            y: sym.height * 0.76
            width: sym.width * 0.22; height: width; radius: width / 2
            color: g.c
        }
        Shape {   // leaf
            anchors.fill: parent
            visible: g.mode === "saver"
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                fillColor: g.c; strokeColor: "transparent"
                startX: sym.width * 0.12; startY: sym.height * 0.88
                PathQuad { x: sym.width * 0.88; y: sym.height * 0.12; controlX: sym.width * 0.04; controlY: sym.height * 0.04 }
                PathQuad { x: sym.width * 0.12; y: sym.height * 0.88; controlX: sym.width * 0.96; controlY: sym.height * 0.96 }
            }
        }
    }
}
