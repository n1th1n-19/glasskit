import QtQuick
import QtQuick.Layouts
import QtQuick.Shapes
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.ksysguard.sensors as Sensors
import org.kde.plasma.private.battery as Bat

PlasmoidItem {
    id: root
    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground
    preferredRepresentation: fullRepresentation

    readonly property var gauges: [
        { id: "cpu/all/usage", label: "CPU", max: 100, unit: "%", color: "#3b82f6" },
        { id: "memory/physical/usedPercent", label: "RAM", max: 100, unit: "%", color: "#a78bfa" },
        { id: "", battery: true, label: "BAT", max: 100, unit: "%", color: "#34d399" },   // from Plasma's battery model, works on any laptop
        { id: "cpu/all/averageTemperature", label: "TEMP", max: 100, unit: "°", color: "#fb923c" }
    ]

    Bat.BatteryControlModel { id: battery }

    fullRepresentation: Rectangle {
        Layout.minimumWidth: 300; Layout.minimumHeight: 140
        Layout.preferredWidth: 360; Layout.preferredHeight: 170
        radius: 28
        color: Qt.rgba(0.08, 0.09, 0.11, 0.86)
        border.color: "#262a33"; border.width: 1

        RowLayout {
            anchors.fill: parent; anchors.margins: 18
            spacing: 8
            Repeater {
                model: root.gauges
                ColumnLayout {
                    Layout.fillWidth: true; Layout.fillHeight: true
                    spacing: 6
                    Sensors.Sensor { id: s; sensorId: modelData.id; updateRateLimit: 2000 }
                    Item {
                        id: ring
                        Layout.fillWidth: true; Layout.fillHeight: true
                        readonly property real size: Math.min(width, height)
                        readonly property real val: modelData.battery ? battery.percent : (s.value || 0)
                        readonly property real frac: Math.max(0, Math.min(1, val / modelData.max))
                        readonly property real stroke: size * 0.13
                        Shape {
                            anchors.centerIn: parent
                            width: ring.size; height: ring.size
                            preferredRendererType: Shape.CurveRenderer
                            ShapePath {   // track
                                strokeColor: "#22262e"; strokeWidth: ring.stroke; fillColor: "transparent"
                                capStyle: ShapePath.RoundCap
                                PathAngleArc {
                                    centerX: ring.size / 2; centerY: ring.size / 2
                                    radiusX: (ring.size - ring.stroke) / 2; radiusY: radiusX
                                    startAngle: -90; sweepAngle: 360
                                }
                            }
                            ShapePath {   // value
                                strokeColor: modelData.color; strokeWidth: ring.stroke; fillColor: "transparent"
                                capStyle: ShapePath.RoundCap
                                PathAngleArc {
                                    centerX: ring.size / 2; centerY: ring.size / 2
                                    radiusX: (ring.size - ring.stroke) / 2; radiusY: radiusX
                                    startAngle: -90; sweepAngle: 360 * ring.frac
                                }
                            }
                        }
                        Text {
                            anchors.centerIn: parent
                            text: Math.round(ring.val) + modelData.unit
                            color: "#f4f5f7"
                            font.family: "Nunito"; font.weight: Font.Bold
                            font.pixelSize: ring.size * 0.24
                            font.features: { "tnum": 1 }
                        }
                    }
                    Text {
                        Layout.alignment: Qt.AlignHCenter
                        text: modelData.label
                        color: "#8b93a3"
                        font.family: "Nunito"; font.weight: Font.Bold; font.pixelSize: 12
                    }
                }
            }
        }
    }
}
