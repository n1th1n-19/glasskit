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
                Kirigami.Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    width: pill.height * 1.15; height: width
                    isMask: true
                    rotation: 90   // material battery is upright; turn terminal to the right
                    color: battery.pluggedIn ? "#34d399" : (battery.percent <= 15 ? "#fb923c" : "#ffffff")
                    source: "battery-" + String(Math.round(battery.percent / 10) * 10).padStart(3, "0") + (battery.pluggedIn ? "-charging" : "")
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
