import QtQuick
import QtQuick.Layouts
import QtQuick.Shapes
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.kirigami as Kirigami
import org.kde.plasma.plasma5support as P5Support

PlasmoidItem {
    id: root
    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground
    preferredRepresentation: fullRepresentation

    readonly property int focusSecs: 25 * 60
    readonly property int breakSecs: 5 * 60
    property bool onBreak: false
    property bool running: false
    property int remaining: focusSecs
    readonly property int phaseLen: onBreak ? breakSecs : focusSecs

    P5Support.DataSource {
        id: sh; engine: "executable"
        onNewData: (source) => disconnectSource(source)
    }

    Timer {
        interval: 1000; repeat: true; running: root.running
        onTriggered: {
            if (--root.remaining > 0) return;
            // phase done: tell the user, flip focus <-> break, wait for start
            sh.connectSource("notify-send -a Focus -i chronometer '"
                + (root.onBreak ? "Break over" : "Focus session done")
                + "' '" + (root.onBreak ? "Back to work." : "Take 5 minutes.") + "' #" + Date.now());
            root.onBreak = !root.onBreak;
            root.remaining = root.phaseLen;
            root.running = false;
        }
    }
    function reset() { running = false; remaining = phaseLen; }

    fullRepresentation: Rectangle {
        Layout.minimumWidth: 300; Layout.minimumHeight: 150
        Layout.preferredWidth: 360; Layout.preferredHeight: 170
        radius: 28
        color: Qt.rgba(0.08, 0.09, 0.11, 0.86)
        border.color: "#262a33"; border.width: 1

        RowLayout {
            anchors.fill: parent; anchors.margins: 18
            spacing: 18

            Item {
                id: ring
                Layout.preferredWidth: parent.height; Layout.preferredHeight: parent.height
                readonly property real stroke: 11
                readonly property real frac: root.remaining / root.phaseLen
                readonly property color tint: root.onBreak ? "#34d399" : "#3b82f6"
                Shape {
                    anchors.fill: parent
                    preferredRendererType: Shape.CurveRenderer
                    ShapePath {
                        strokeColor: "#22262e"; strokeWidth: ring.stroke; fillColor: "transparent"; capStyle: ShapePath.RoundCap
                        PathAngleArc { centerX: ring.width / 2; centerY: ring.height / 2; radiusX: (ring.width - ring.stroke) / 2; radiusY: radiusX; startAngle: -90; sweepAngle: 360 }
                    }
                    ShapePath {
                        strokeColor: ring.tint; strokeWidth: ring.stroke; fillColor: "transparent"; capStyle: ShapePath.RoundCap
                        PathAngleArc { centerX: ring.width / 2; centerY: ring.height / 2; radiusX: (ring.width - ring.stroke) / 2; radiusY: radiusX; startAngle: -90; sweepAngle: 360 * ring.frac }
                    }
                }
                Text {
                    anchors.centerIn: parent
                    text: Math.floor(root.remaining / 60) + ":" + String(root.remaining % 60).padStart(2, "0")
                    color: "#f4f5f7"
                    font.family: "Nunito"; font.weight: Font.Black; font.pixelSize: ring.width * 0.24
                    font.features: { "tnum": 1 }
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 10
                Text {
                    text: root.onBreak ? "BREAK" : "FOCUS"
                    color: ring.tint
                    font.family: "Nunito"; font.weight: Font.Black; font.pixelSize: 14; font.letterSpacing: 1.5
                }
                Text {
                    Layout.fillWidth: true; wrapMode: Text.WordWrap
                    text: root.running ? (root.onBreak ? "Rest your eyes." : "Stay on one thing.")
                                       : (root.remaining === root.phaseLen ? "Ready when you are." : "Paused.")
                    color: "#8b93a3"; font.family: "Nunito"; font.weight: Font.Bold; font.pixelSize: 13
                }
                RowLayout {
                    spacing: 8
                    Repeater {
                        model: [
                            { icon: root.running ? "media-playback-pause" : "media-playback-start", act: "toggle", main: true },
                            { icon: "edit-undo", act: "reset", main: false },
                            { icon: "media-skip-forward", act: "skip", main: false }
                        ]
                        Rectangle {
                            width: modelData.main ? 44 : 36; height: width; radius: width / 2
                            color: modelData.main ? ring.tint : (hov.containsMouse ? "#2a2f39" : "#22262e")
                            Kirigami.Icon { anchors.centerIn: parent; width: parent.width * 0.5; height: width; source: modelData.icon; isMask: true; color: "#ffffff" }
                            MouseArea {
                                id: hov; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    if (modelData.act === "toggle") root.running = !root.running;
                                    else if (modelData.act === "reset") root.reset();
                                    else { root.onBreak = !root.onBreak; root.reset(); }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
