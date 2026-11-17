import QtQuick
import org.kde.plasma.plasma5support as P5Support
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

    P5Support.DataSource {   // battery saver = power-profiles-daemon "power-saver" profile, polled every 5 s

        id: saverSource; engine: "executable"

        property bool on: false

        onNewData: (source, data) => { on = (data.stdout || "").indexOf("power-saver") >= 0; disconnectSource(source); }

    }

    Timer { interval: 5000; repeat: true; running: true; triggeredOnStart: true

        onTriggered: saverSource.connectSource("gdbus call --system --dest net.hadess.PowerProfiles --object-path /net/hadess/PowerProfiles --method org.freedesktop.DBus.Properties.Get net.hadess.PowerProfiles ActiveProfile #" + Date.now()) }

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
                spacing: Math.round(pill.height * 0.22)   // gap between battery and percent
                BatteryGlyph {
                    anchors.verticalCenter: parent.verticalCenter
                    height: pill.height * 0.48
                    width: height * 1.9
                    percent: battery.percent
                    charging: battery.pluggedIn
                    saver: saverSource.on
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
