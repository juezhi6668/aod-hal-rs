/**
 * 从 ROM 内 SystemUI 的 IDisplayPanelFeature$Stub / $Stub$Proxy 逆向还原。
 *
 * 事务码（= 声明顺序，不可调整）：
 *   1 -> getDisplayPanelFeatureValue(int featureId, out int[] values)
 *   2 -> setDisplayPanelFeatureValue(int featureId, in  int[] values)
 *   3 -> getDisplayPanelInfo(int displayId, out String[] info)
 *
 * 注意 values 是 out 而不是 inout：
 *   Proxy.getDisplayPanelFeatureValue() 只写了 length（null 时为 -1），
 *   没有写元素数据；Stub 侧也是 new int[len] 之后直接回调实现。
 *   写成 inout 会让 parcel 格式对不上。
 *
 * 回复格式（由 Stub.onTransact 实测）：
 *   code 1 : status(0) + int(返回值) + intArray(values)
 *   code 2 : status(0) + int(返回值)
 *   code 3 : status(0) + int(返回值) + String[](info)
 */
package vendor.oplus.hardware.displaypanelfeature;

@VintfStability
interface IDisplayPanelFeature {
    int getDisplayPanelFeatureValue(int featureId, out int[] values);
    int setDisplayPanelFeatureValue(int featureId, in int[] values);
    int getDisplayPanelInfo(int displayId, out String[] info);
}
