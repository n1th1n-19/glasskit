import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.clock as PlasmaClock

PlasmoidItem {
    id: root
    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground
    preferredRepresentation: fullRepresentation

    fullRepresentation: Rectangle {
        Layout.minimumWidth: 300; Layout.minimumHeight: 150
        Layout.preferredWidth: 360; Layout.preferredHeight: 170
        radius: 28
        color: Qt.rgba(0.08, 0.09, 0.11, 0.86)
        border.color: "#262a33"; border.width: 1

        PlasmaClock.Clock { id: clock; trackSeconds: false }
        readonly property date now: clock.dateTime
        // share of the day already gone, 0..1
        readonly property real dayDone: (now.getHours() * 60 + now.getMinutes()) / 1440

        ColumnLayout {
            anchors.fill: parent; anchors.margins: 22
            spacing: 6
            Text {
                text: Qt.formatTime(parent.parent.now, "h:mm")
                color: "#f4f5f7"
                font.family: "Nunito"; font.weight: Font.Black
                font.pixelSize: parent.height * 0.46
                font.features: { "tnum": 1 }
            }
            Text {
                text: Qt.formatDate(parent.parent.now, "dddd, d MMMM")
                color: "#aab1bf"
                font.family: "Nunito"; font.weight: Font.Bold; font.pixelSize: 15
            }
            Item { Layout.fillHeight: true }
            Rectangle {   // day progress track
                Layout.fillWidth: true; height: 8; radius: 4; color: "#22262e"
                Rectangle {
                    height: parent.height; radius: 4; color: "#3b82f6"
                    width: Math.max(height, parent.width * parent.parent.parent.dayDone)
                }
            }
        }
    }
}
