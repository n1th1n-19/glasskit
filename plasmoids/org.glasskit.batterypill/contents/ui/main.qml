import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.kirigami as Kirigami
import org.kde.plasma.private.battery as Bat

// same pill as the lock screen: battery icon + percent, green while charging
PlasmoidItem {
    id: root
    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground
    preferredRepresentation: fullRepresentation

    Bat.BatteryControlModel { id: battery }

    fullRepresentation: Item {
        readonly property real pillH: height
        Layout.minimumWidth: pill.width + 4
        Layout.preferredWidth: pill.width + 4
        visible: battery.hasInternalBatteries

        Rectangle {
            id: pill
            anchors.centerIn: parent
            height: parent.pillH
            width: row.implicitWidth + 4
            radius: height / 2
            color: "transparent"

            Row {
                id: row
                anchors.centerIn: parent
                spacing: 0
                Item {   // battery drawn from shapes: always horizontal, terminal on the right, fill = charge level
                    anchors.verticalCenter: parent.verticalCenter
                    height: pill.height * 0.48
                    width: height * 1.9
                    readonly property color c: battery.pluggedIn ? "#34d399" : (battery.percent <= 15 ? "#fb923c" : "#ffffff")
                    readonly property real bw: Math.max(1.5, height * 0.12)
                    Rectangle { id: batBody; width: parent.width - parent.height * 0.2; height: parent.height; radius: height * 0.3; color: "transparent"; border.width: parent.bw; border.color: parent.c }
                    Rectangle { x: batBody.width + parent.height * 0.05; anchors.verticalCenter: parent.verticalCenter; width: parent.height * 0.14; height: parent.height * 0.4; radius: width / 2; color: parent.c }
                    Rectangle {
                        x: parent.bw * 1.8; y: parent.bw * 1.8; height: parent.height - parent.bw * 3.6; radius: height * 0.3; color: parent.c
                        width: Math.max(height * 0.5, (batBody.width - parent.bw * 3.6) * Math.min(1, battery.percent / 100))
                    }
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: battery.percent + "%"
                    color: "#ffffff"
                    font.family: "Nunito"; font.weight: Font.Black; font.pixelSize: Math.round(pill.height * 0.6)
                    font.features: { "tnum": 1 }
                }
            }
        }
    }
}
