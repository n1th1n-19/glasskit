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
        id: card
        Layout.minimumWidth: 300; Layout.minimumHeight: 260
        Layout.preferredWidth: 360; Layout.preferredHeight: 300
        radius: 28
        color: Qt.rgba(0.08, 0.09, 0.11, 0.86)
        border.color: "#262a33"; border.width: 1

        PlasmaClock.Clock { id: clock; trackSeconds: false }
        readonly property date now: clock.dateTime
        readonly property int year: now.getFullYear()
        readonly property int month: now.getMonth()
        readonly property int today: now.getDate()
        // Monday-first offset of day 1, and number of days in the month
        readonly property int lead: (new Date(year, month, 1).getDay() + 6) % 7
        readonly property int days: new Date(year, month + 1, 0).getDate()

        ColumnLayout {
            anchors.fill: parent; anchors.margins: 20
            spacing: 10
            Text {
                text: Qt.formatDate(card.now, "MMMM yyyy")
                color: "#f4f5f7"
                font.family: "Nunito"; font.weight: Font.Bold; font.pixelSize: 20
            }
            GridLayout {
                Layout.fillWidth: true; Layout.fillHeight: true
                columns: 7; rowSpacing: 2; columnSpacing: 2
                Repeater {
                    model: ["M", "T", "W", "T", "F", "S", "S"]
                    Text {
                        Layout.fillWidth: true
                        horizontalAlignment: Text.AlignHCenter
                        text: modelData
                        color: index >= 5 ? "#6b7383" : "#8b93a3"
                        font.family: "Nunito"; font.weight: Font.Bold; font.pixelSize: 12
                    }
                }
                Repeater {
                    model: card.lead + card.days
                    Item {
                        readonly property int day: index - card.lead + 1
                        readonly property bool isToday: day === card.today
                        Layout.fillWidth: true; Layout.fillHeight: true
                        Rectangle {
                            anchors.centerIn: parent
                            width: Math.min(parent.width, parent.height) - 2; height: width
                            radius: width / 2
                            color: "#3b82f6"
                            visible: parent.isToday
                        }
                        Text {
                            anchors.centerIn: parent
                            text: parent.day > 0 ? parent.day : ""
                            color: parent.isToday ? "#ffffff" : "#d6dae2"
                            font.family: "Nunito"; font.weight: Font.Bold; font.pixelSize: 14
                            font.features: { "tnum": 1 }
                        }
                    }
                }
            }
        }
    }
}
