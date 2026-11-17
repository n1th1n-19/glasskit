import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.kirigami as Kirigami
import org.kde.ksysguard.sensors as Sensors

PlasmoidItem {
    id: root
    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground
    preferredRepresentation: fullRepresentation

    Sensors.Sensor { id: used; sensorId: "disk/all/used"; updateRateLimit: 10000 }
    Sensors.Sensor { id: total; sensorId: "disk/all/total"; updateRateLimit: 10000 }
    Sensors.Sensor { id: free; sensorId: "disk/all/free"; updateRateLimit: 10000 }
    Sensors.Sensor { id: pct; sensorId: "disk/all/usedPercent"; updateRateLimit: 10000 }
    Sensors.Sensor { id: down; sensorId: "network/all/download"; updateRateLimit: 1000 }
    Sensors.Sensor { id: up; sensorId: "network/all/upload"; updateRateLimit: 1000 }

    fullRepresentation: Rectangle {
        Layout.minimumWidth: 300; Layout.minimumHeight: 140
        Layout.preferredWidth: 360; Layout.preferredHeight: 160
        radius: 28
        color: Qt.rgba(0.08, 0.09, 0.11, 0.86)
        border.color: "#262a33"; border.width: 1

        ColumnLayout {
            anchors.fill: parent; anchors.margins: 20
            spacing: 10

            RowLayout {
                Layout.fillWidth: true
                Text {
                    text: Math.round(pct.value || 0) + "%"
                    color: "#f4f5f7"
                    font.family: "Nunito"; font.weight: Font.Black; font.pixelSize: 34
                    font.features: { "tnum": 1 }
                }
                ColumnLayout {
                    Layout.fillWidth: true; spacing: 0
                    Text { text: "Storage used"; color: "#8b93a3"; font.family: "Nunito"; font.weight: Font.Bold; font.pixelSize: 12 }
                    Text {
                        Layout.fillWidth: true; elide: Text.ElideRight
                        text: free.formattedValue + " free of " + total.formattedValue
                        color: "#d6dae2"; font.family: "Nunito"; font.weight: Font.Bold; font.pixelSize: 14
                    }
                }
            }
            Rectangle {   // usage bar, turns orange past 85%
                Layout.fillWidth: true; height: 10; radius: 5; color: "#22262e"
                Rectangle {
                    height: parent.height; radius: 5
                    width: Math.max(height, parent.width * Math.min(1, (pct.value || 0) / 100))
                    color: (pct.value || 0) > 85 ? "#fb923c" : "#3b82f6"
                }
            }
            Item { Layout.fillHeight: true }
            RowLayout {
                Layout.fillWidth: true
                spacing: 18
                Repeater {
                    model: [
                        { icon: "download", value: down.formattedValue, color: "#34d399" },
                        { icon: "upload", value: up.formattedValue, color: "#a78bfa" }
                    ]
                    RowLayout {
                        spacing: 6
                        Kirigami.Icon { width: 18; height: 18; source: modelData.icon; isMask: true; color: modelData.color }
                        Text {
                            text: modelData.value || "0 B/s"
                            color: "#f4f5f7"; font.family: "Nunito"; font.weight: Font.Bold; font.pixelSize: 15
                            font.features: { "tnum": 1 }
                        }
                    }
                }
            }
        }
    }
}
