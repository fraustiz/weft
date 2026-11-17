#[inline]
pub unsafe fn CoCreateInstance<P1, T>(
    rclsid: *const windows_core::GUID,
    punkouter: P1,
    dwclscontext: u32,
) -> windows_core::Result<T>
where
    P1: windows_core::Param<windows_core::IUnknown>,
    T: windows_core::Interface,
{
    windows_core::link!("ole32.dll" "system" fn CoCreateInstance(rclsid : *const windows_core::GUID, punkouter : *mut core::ffi::c_void, dwclscontext : u32, riid : *const windows_core::GUID, ppv : *mut *mut core::ffi::c_void) -> windows_core::HRESULT);
    let mut result__ = core::ptr::null_mut();
    unsafe {
        CoCreateInstance(
            rclsid,
            punkouter.param().abi(),
            dwclscontext,
            &T::IID,
            &mut result__,
        )
        .and_then(|| windows_core::imp::Type::from_abi(result__))
    }
}
#[inline]
pub unsafe fn CoInitializeEx(
    pvreserved: Option<*const core::ffi::c_void>,
    dwcoinit: u32,
) -> windows_core::HRESULT {
    windows_core::link!("ole32.dll" "system" fn CoInitializeEx(pvreserved : *const core::ffi::c_void, dwcoinit : u32) -> windows_core::HRESULT);
    unsafe { CoInitializeEx(pvreserved.unwrap_or(core::mem::zeroed()) as _, dwcoinit) }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BLOB {
    pub cbSize: u32,
    pub pBlobData: *mut u8,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BSTRBLOB {
    pub cbSize: u32,
    pub pData: *mut u8,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CABOOL {
    pub cElems: u32,
    pub pElems: *mut VARIANT_BOOL,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CABSTR {
    pub cElems: u32,
    pub pElems: *mut windows_core::BSTR,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CABSTRBLOB {
    pub cElems: u32,
    pub pElems: *mut BSTRBLOB,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAC {
    pub cElems: u32,
    pub pElems: *mut i8,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CACLIPDATA {
    pub cElems: u32,
    pub pElems: *mut CLIPDATA,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CACLSID {
    pub cElems: u32,
    pub pElems: *mut windows_core::GUID,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CACY {
    pub cElems: u32,
    pub pElems: *mut CY,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CADATE {
    pub cElems: u32,
    pub pElems: *mut f64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CADBL {
    pub cElems: u32,
    pub pElems: *mut f64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAFILETIME {
    pub cElems: u32,
    pub pElems: *mut FILETIME,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAFLT {
    pub cElems: u32,
    pub pElems: *mut f32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAH {
    pub cElems: u32,
    pub pElems: *mut i64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAI {
    pub cElems: u32,
    pub pElems: *mut i16,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAL {
    pub cElems: u32,
    pub pElems: *mut i32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CALPSTR {
    pub cElems: u32,
    pub pElems: *mut windows_core::PSTR,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CALPWSTR {
    pub cElems: u32,
    pub pElems: *mut windows_core::PWSTR,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAPROPVARIANT {
    pub cElems: u32,
    pub pElems: *mut PROPVARIANT,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CASCODE {
    pub cElems: u32,
    pub pElems: *mut SCODE,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAUB {
    pub cElems: u32,
    pub pElems: *mut u8,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAUH {
    pub cElems: u32,
    pub pElems: *mut u64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAUI {
    pub cElems: u32,
    pub pElems: *mut u16,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAUL {
    pub cElems: u32,
    pub pElems: *mut u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CLIPDATA {
    pub cbSize: u32,
    pub ulClipFmt: i32,
    pub pClipData: *mut u8,
}
pub type CLSCTX = u32;
pub type COINIT = i32;
pub const CUIAutomation: windows_core::GUID =
    windows_core::GUID::from_u128(0xff48dba4_60ef_4201_aa87_54103eef594e);
#[repr(C)]
#[derive(Clone, Copy)]
pub union CY {
    pub Anonymous: CY_0,
    pub int64: i64,
}
impl Default for CY {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CY_0 {
    pub Lo: u32,
    pub Hi: i32,
}
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentSessionChangedEventArgs(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    CurrentSessionChangedEventArgs,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl windows_core::RuntimeType for CurrentSessionChangedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_class::<Self, ICurrentSessionChangedEventArgs>();
}
unsafe impl windows_core::Interface for CurrentSessionChangedEventArgs {
    type Vtable = <ICurrentSessionChangedEventArgs as windows_core::Interface>::Vtable;
    const IID: windows_core::GUID =
        <ICurrentSessionChangedEventArgs as windows_core::Interface>::IID;
}
impl windows_core::RuntimeName for CurrentSessionChangedEventArgs {
    const NAME: &'static str = "Windows.Media.Control.CurrentSessionChangedEventArgs";
}
unsafe impl Send for CurrentSessionChangedEventArgs {}
unsafe impl Sync for CurrentSessionChangedEventArgs {}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DECIMAL {
    pub wReserved: u16,
    pub Anonymous: DECIMAL_0,
    pub Hi32: u32,
    pub Anonymous2: DECIMAL_1,
}
impl Default for DECIMAL {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union DECIMAL_0 {
    pub Anonymous: DECIMAL_0_0,
    pub signscale: u16,
}
impl Default for DECIMAL_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DECIMAL_0_0 {
    pub scale: u8,
    pub sign: u8,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union DECIMAL_1 {
    pub Anonymous: DECIMAL_1_0,
    pub Lo64: u64,
}
impl Default for DECIMAL_1 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DECIMAL_1_0 {
    pub Lo32: u32,
    pub Mid32: u32,
}
pub type EDataFlow = i32;
pub type ERole = i32;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FILETIME {
    pub dwLowDateTime: u32,
    pub dwHighDateTime: u32,
}
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalSystemMediaTransportControlsSession(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    GlobalSystemMediaTransportControlsSession,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl GlobalSystemMediaTransportControlsSession {
    pub fn SourceAppUserModelId(&self) -> windows_core::Result<windows_core::HSTRING> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).SourceAppUserModelId)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn TryGetMediaPropertiesAsync(
        &self,
    ) -> windows_core::Result<
        windows_future::IAsyncOperation<GlobalSystemMediaTransportControlsSessionMediaProperties>,
    > {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).TryGetMediaPropertiesAsync)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn GetTimelineProperties(
        &self,
    ) -> windows_core::Result<GlobalSystemMediaTransportControlsSessionTimelineProperties> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetTimelineProperties)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn GetPlaybackInfo(
        &self,
    ) -> windows_core::Result<GlobalSystemMediaTransportControlsSessionPlaybackInfo> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetPlaybackInfo)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn TryPlayAsync(&self) -> windows_core::Result<windows_future::IAsyncOperation<bool>> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).TryPlayAsync)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn TryPauseAsync(&self) -> windows_core::Result<windows_future::IAsyncOperation<bool>> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).TryPauseAsync)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn TryStopAsync(&self) -> windows_core::Result<windows_future::IAsyncOperation<bool>> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).TryStopAsync)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn TryRecordAsync(&self) -> windows_core::Result<windows_future::IAsyncOperation<bool>> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).TryRecordAsync)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn TryFastForwardAsync(
        &self,
    ) -> windows_core::Result<windows_future::IAsyncOperation<bool>> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).TryFastForwardAsync)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn TryRewindAsync(&self) -> windows_core::Result<windows_future::IAsyncOperation<bool>> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).TryRewindAsync)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn TrySkipNextAsync(&self) -> windows_core::Result<windows_future::IAsyncOperation<bool>> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).TrySkipNextAsync)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn TrySkipPreviousAsync(
        &self,
    ) -> windows_core::Result<windows_future::IAsyncOperation<bool>> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).TrySkipPreviousAsync)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn TryChangeChannelUpAsync(
        &self,
    ) -> windows_core::Result<windows_future::IAsyncOperation<bool>> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).TryChangeChannelUpAsync)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn TryChangeChannelDownAsync(
        &self,
    ) -> windows_core::Result<windows_future::IAsyncOperation<bool>> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).TryChangeChannelDownAsync)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn TryTogglePlayPauseAsync(
        &self,
    ) -> windows_core::Result<windows_future::IAsyncOperation<bool>> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).TryTogglePlayPauseAsync)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn TryChangeAutoRepeatModeAsync(
        &self,
        requestedautorepeatmode: MediaPlaybackAutoRepeatMode,
    ) -> windows_core::Result<windows_future::IAsyncOperation<bool>> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).TryChangeAutoRepeatModeAsync)(
                windows_core::Interface::as_raw(self),
                requestedautorepeatmode,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn TryChangePlaybackRateAsync(
        &self,
        requestedplaybackrate: f64,
    ) -> windows_core::Result<windows_future::IAsyncOperation<bool>> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).TryChangePlaybackRateAsync)(
                windows_core::Interface::as_raw(self),
                requestedplaybackrate,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn TryChangeShuffleActiveAsync(
        &self,
        requestedshufflestate: bool,
    ) -> windows_core::Result<windows_future::IAsyncOperation<bool>> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).TryChangeShuffleActiveAsync)(
                windows_core::Interface::as_raw(self),
                requestedshufflestate,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn TryChangePlaybackPositionAsync(
        &self,
        requestedplaybackposition: i64,
    ) -> windows_core::Result<windows_future::IAsyncOperation<bool>> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).TryChangePlaybackPositionAsync)(
                windows_core::Interface::as_raw(self),
                requestedplaybackposition,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn TimelinePropertiesChanged<F>(
        &self,
        handler: F,
    ) -> windows_core::Result<windows_core::EventRevoker>
    where
        F: Fn(windows_core::Ref<Self>, windows_core::Ref<TimelinePropertiesChangedEventArgs>)
            + Send
            + 'static,
    {
        let handler =
            <TypedEventHandler<Self, TimelinePropertiesChangedEventArgs>>::new(move |a0, a1| {
                handler(a0, a1);
                Ok(())
            });
        unsafe {
            let mut result__ = core::mem::zeroed();
            let token__ = (windows_core::Interface::vtable(self).TimelinePropertiesChanged)(
                windows_core::Interface::as_raw(self),
                windows_core::Interface::as_raw(&handler),
                &mut result__,
            )
            .map(|| result__)?;
            Ok(windows_core::EventRevoker::new(
                self.clone(),
                token__,
                windows_core::Interface::vtable(self).RemoveTimelinePropertiesChanged,
            ))
        }
    }
    pub fn PlaybackInfoChanged<F>(
        &self,
        handler: F,
    ) -> windows_core::Result<windows_core::EventRevoker>
    where
        F: Fn(windows_core::Ref<Self>, windows_core::Ref<PlaybackInfoChangedEventArgs>)
            + Send
            + 'static,
    {
        let handler =
            <TypedEventHandler<Self, PlaybackInfoChangedEventArgs>>::new(move |a0, a1| {
                handler(a0, a1);
                Ok(())
            });
        unsafe {
            let mut result__ = core::mem::zeroed();
            let token__ = (windows_core::Interface::vtable(self).PlaybackInfoChanged)(
                windows_core::Interface::as_raw(self),
                windows_core::Interface::as_raw(&handler),
                &mut result__,
            )
            .map(|| result__)?;
            Ok(windows_core::EventRevoker::new(
                self.clone(),
                token__,
                windows_core::Interface::vtable(self).RemovePlaybackInfoChanged,
            ))
        }
    }
    pub fn MediaPropertiesChanged<F>(
        &self,
        handler: F,
    ) -> windows_core::Result<windows_core::EventRevoker>
    where
        F: Fn(windows_core::Ref<Self>, windows_core::Ref<MediaPropertiesChangedEventArgs>)
            + Send
            + 'static,
    {
        let handler =
            <TypedEventHandler<Self, MediaPropertiesChangedEventArgs>>::new(move |a0, a1| {
                handler(a0, a1);
                Ok(())
            });
        unsafe {
            let mut result__ = core::mem::zeroed();
            let token__ = (windows_core::Interface::vtable(self).MediaPropertiesChanged)(
                windows_core::Interface::as_raw(self),
                windows_core::Interface::as_raw(&handler),
                &mut result__,
            )
            .map(|| result__)?;
            Ok(windows_core::EventRevoker::new(
                self.clone(),
                token__,
                windows_core::Interface::vtable(self).RemoveMediaPropertiesChanged,
            ))
        }
    }
}
impl windows_core::RuntimeType for GlobalSystemMediaTransportControlsSession {
    const SIGNATURE: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::for_class::<
        Self,
        IGlobalSystemMediaTransportControlsSession,
    >();
}
unsafe impl windows_core::Interface for GlobalSystemMediaTransportControlsSession {
    type Vtable = <IGlobalSystemMediaTransportControlsSession as windows_core::Interface>::Vtable;
    const IID: windows_core::GUID =
        <IGlobalSystemMediaTransportControlsSession as windows_core::Interface>::IID;
}
impl windows_core::RuntimeName for GlobalSystemMediaTransportControlsSession {
    const NAME: &'static str = "Windows.Media.Control.GlobalSystemMediaTransportControlsSession";
}
unsafe impl Send for GlobalSystemMediaTransportControlsSession {}
unsafe impl Sync for GlobalSystemMediaTransportControlsSession {}
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalSystemMediaTransportControlsSessionManager(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    GlobalSystemMediaTransportControlsSessionManager,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl GlobalSystemMediaTransportControlsSessionManager {
    pub fn GetCurrentSession(
        &self,
    ) -> windows_core::Result<GlobalSystemMediaTransportControlsSession> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetCurrentSession)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn GetSessions(
        &self,
    ) -> windows_core::Result<
        windows_collections::IVectorView<GlobalSystemMediaTransportControlsSession>,
    > {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetSessions)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn CurrentSessionChanged<F>(
        &self,
        handler: F,
    ) -> windows_core::Result<windows_core::EventRevoker>
    where
        F: Fn(windows_core::Ref<Self>, windows_core::Ref<CurrentSessionChangedEventArgs>)
            + Send
            + 'static,
    {
        let handler =
            <TypedEventHandler<Self, CurrentSessionChangedEventArgs>>::new(move |a0, a1| {
                handler(a0, a1);
                Ok(())
            });
        unsafe {
            let mut result__ = core::mem::zeroed();
            let token__ = (windows_core::Interface::vtable(self).CurrentSessionChanged)(
                windows_core::Interface::as_raw(self),
                windows_core::Interface::as_raw(&handler),
                &mut result__,
            )
            .map(|| result__)?;
            Ok(windows_core::EventRevoker::new(
                self.clone(),
                token__,
                windows_core::Interface::vtable(self).RemoveCurrentSessionChanged,
            ))
        }
    }
    pub fn SessionsChanged<F>(&self, handler: F) -> windows_core::Result<windows_core::EventRevoker>
    where
        F: Fn(windows_core::Ref<Self>, windows_core::Ref<SessionsChangedEventArgs>)
            + Send
            + 'static,
    {
        let handler = <TypedEventHandler<Self, SessionsChangedEventArgs>>::new(move |a0, a1| {
            handler(a0, a1);
            Ok(())
        });
        unsafe {
            let mut result__ = core::mem::zeroed();
            let token__ = (windows_core::Interface::vtable(self).SessionsChanged)(
                windows_core::Interface::as_raw(self),
                windows_core::Interface::as_raw(&handler),
                &mut result__,
            )
            .map(|| result__)?;
            Ok(windows_core::EventRevoker::new(
                self.clone(),
                token__,
                windows_core::Interface::vtable(self).RemoveSessionsChanged,
            ))
        }
    }
    pub fn RequestAsync() -> windows_core::Result<windows_future::IAsyncOperation<Self>> {
        Self::IGlobalSystemMediaTransportControlsSessionManagerStatics(|this| unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).RequestAsync)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        })
    }
    fn IGlobalSystemMediaTransportControlsSessionManagerStatics<
        R,
        F: FnOnce(
            &IGlobalSystemMediaTransportControlsSessionManagerStatics,
        ) -> windows_core::Result<R>,
    >(
        callback: F,
    ) -> windows_core::Result<R> {
        static SHARED: windows_core::imp::FactoryCache<
            GlobalSystemMediaTransportControlsSessionManager,
            IGlobalSystemMediaTransportControlsSessionManagerStatics,
        > = windows_core::imp::FactoryCache::new();
        SHARED.call(callback)
    }
}
impl windows_core::RuntimeType for GlobalSystemMediaTransportControlsSessionManager {
    const SIGNATURE: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::for_class::<
        Self,
        IGlobalSystemMediaTransportControlsSessionManager,
    >();
}
unsafe impl windows_core::Interface for GlobalSystemMediaTransportControlsSessionManager {
    type Vtable =
        <IGlobalSystemMediaTransportControlsSessionManager as windows_core::Interface>::Vtable;
    const IID: windows_core::GUID =
        <IGlobalSystemMediaTransportControlsSessionManager as windows_core::Interface>::IID;
}
impl windows_core::RuntimeName for GlobalSystemMediaTransportControlsSessionManager {
    const NAME: &'static str =
        "Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager";
}
unsafe impl Send for GlobalSystemMediaTransportControlsSessionManager {}
unsafe impl Sync for GlobalSystemMediaTransportControlsSessionManager {}
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalSystemMediaTransportControlsSessionMediaProperties(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    GlobalSystemMediaTransportControlsSessionMediaProperties,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl GlobalSystemMediaTransportControlsSessionMediaProperties {
    pub fn Title(&self) -> windows_core::Result<windows_core::HSTRING> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).Title)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn Subtitle(&self) -> windows_core::Result<windows_core::HSTRING> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).Subtitle)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn AlbumArtist(&self) -> windows_core::Result<windows_core::HSTRING> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).AlbumArtist)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn Artist(&self) -> windows_core::Result<windows_core::HSTRING> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).Artist)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn AlbumTitle(&self) -> windows_core::Result<windows_core::HSTRING> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).AlbumTitle)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn TrackNumber(&self) -> windows_core::Result<i32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).TrackNumber)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn Genres(
        &self,
    ) -> windows_core::Result<windows_collections::IVectorView<windows_core::HSTRING>> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).Genres)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn AlbumTrackCount(&self) -> windows_core::Result<i32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).AlbumTrackCount)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn PlaybackType(&self) -> windows_core::Result<MediaPlaybackType> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).PlaybackType)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
            .and_then(|r__: windows_reference::IReference<MediaPlaybackType>| r__.Value())
        }
    }
}
impl windows_core::RuntimeType for GlobalSystemMediaTransportControlsSessionMediaProperties {
    const SIGNATURE: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::for_class::<
        Self,
        IGlobalSystemMediaTransportControlsSessionMediaProperties,
    >();
}
unsafe impl windows_core::Interface for GlobalSystemMediaTransportControlsSessionMediaProperties {
    type Vtable = < IGlobalSystemMediaTransportControlsSessionMediaProperties as windows_core::Interface >::Vtable ;
    const IID: windows_core::GUID =
        <IGlobalSystemMediaTransportControlsSessionMediaProperties as windows_core::Interface>::IID;
}
impl windows_core::RuntimeName for GlobalSystemMediaTransportControlsSessionMediaProperties {
    const NAME: &'static str =
        "Windows.Media.Control.GlobalSystemMediaTransportControlsSessionMediaProperties";
}
unsafe impl Send for GlobalSystemMediaTransportControlsSessionMediaProperties {}
unsafe impl Sync for GlobalSystemMediaTransportControlsSessionMediaProperties {}
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalSystemMediaTransportControlsSessionPlaybackControls(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    GlobalSystemMediaTransportControlsSessionPlaybackControls,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl GlobalSystemMediaTransportControlsSessionPlaybackControls {
    pub fn IsPlayEnabled(&self) -> windows_core::Result<bool> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).IsPlayEnabled)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn IsPauseEnabled(&self) -> windows_core::Result<bool> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).IsPauseEnabled)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn IsStopEnabled(&self) -> windows_core::Result<bool> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).IsStopEnabled)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn IsRecordEnabled(&self) -> windows_core::Result<bool> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).IsRecordEnabled)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn IsFastForwardEnabled(&self) -> windows_core::Result<bool> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).IsFastForwardEnabled)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn IsRewindEnabled(&self) -> windows_core::Result<bool> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).IsRewindEnabled)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn IsNextEnabled(&self) -> windows_core::Result<bool> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).IsNextEnabled)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn IsPreviousEnabled(&self) -> windows_core::Result<bool> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).IsPreviousEnabled)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn IsChannelUpEnabled(&self) -> windows_core::Result<bool> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).IsChannelUpEnabled)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn IsChannelDownEnabled(&self) -> windows_core::Result<bool> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).IsChannelDownEnabled)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn IsPlayPauseToggleEnabled(&self) -> windows_core::Result<bool> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).IsPlayPauseToggleEnabled)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn IsShuffleEnabled(&self) -> windows_core::Result<bool> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).IsShuffleEnabled)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn IsRepeatEnabled(&self) -> windows_core::Result<bool> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).IsRepeatEnabled)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn IsPlaybackRateEnabled(&self) -> windows_core::Result<bool> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).IsPlaybackRateEnabled)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn IsPlaybackPositionEnabled(&self) -> windows_core::Result<bool> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).IsPlaybackPositionEnabled)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
}
impl windows_core::RuntimeType for GlobalSystemMediaTransportControlsSessionPlaybackControls {
    const SIGNATURE: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::for_class::<
        Self,
        IGlobalSystemMediaTransportControlsSessionPlaybackControls,
    >();
}
unsafe impl windows_core::Interface for GlobalSystemMediaTransportControlsSessionPlaybackControls {
    type Vtable = < IGlobalSystemMediaTransportControlsSessionPlaybackControls as windows_core::Interface >::Vtable ;
    const IID : windows_core::GUID = < IGlobalSystemMediaTransportControlsSessionPlaybackControls as windows_core::Interface >::IID ;
}
impl windows_core::RuntimeName for GlobalSystemMediaTransportControlsSessionPlaybackControls {
    const NAME: &'static str =
        "Windows.Media.Control.GlobalSystemMediaTransportControlsSessionPlaybackControls";
}
unsafe impl Send for GlobalSystemMediaTransportControlsSessionPlaybackControls {}
unsafe impl Sync for GlobalSystemMediaTransportControlsSessionPlaybackControls {}
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalSystemMediaTransportControlsSessionPlaybackInfo(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    GlobalSystemMediaTransportControlsSessionPlaybackInfo,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl GlobalSystemMediaTransportControlsSessionPlaybackInfo {
    pub fn Controls(
        &self,
    ) -> windows_core::Result<GlobalSystemMediaTransportControlsSessionPlaybackControls> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).Controls)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn PlaybackStatus(
        &self,
    ) -> windows_core::Result<GlobalSystemMediaTransportControlsSessionPlaybackStatus> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).PlaybackStatus)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn PlaybackType(&self) -> windows_core::Result<MediaPlaybackType> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).PlaybackType)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
            .and_then(|r__: windows_reference::IReference<MediaPlaybackType>| r__.Value())
        }
    }
    pub fn AutoRepeatMode(&self) -> windows_core::Result<MediaPlaybackAutoRepeatMode> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).AutoRepeatMode)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
            .and_then(|r__: windows_reference::IReference<MediaPlaybackAutoRepeatMode>| r__.Value())
        }
    }
    pub fn PlaybackRate(&self) -> windows_core::Result<f64> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).PlaybackRate)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
            .and_then(|r__: windows_reference::IReference<f64>| r__.Value())
        }
    }
    pub fn IsShuffleActive(&self) -> windows_core::Result<bool> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).IsShuffleActive)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
            .and_then(|r__: windows_reference::IReference<bool>| r__.Value())
        }
    }
}
impl windows_core::RuntimeType for GlobalSystemMediaTransportControlsSessionPlaybackInfo {
    const SIGNATURE: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::for_class::<
        Self,
        IGlobalSystemMediaTransportControlsSessionPlaybackInfo,
    >();
}
unsafe impl windows_core::Interface for GlobalSystemMediaTransportControlsSessionPlaybackInfo {
    type Vtable =
        <IGlobalSystemMediaTransportControlsSessionPlaybackInfo as windows_core::Interface>::Vtable;
    const IID: windows_core::GUID =
        <IGlobalSystemMediaTransportControlsSessionPlaybackInfo as windows_core::Interface>::IID;
}
impl windows_core::RuntimeName for GlobalSystemMediaTransportControlsSessionPlaybackInfo {
    const NAME: &'static str =
        "Windows.Media.Control.GlobalSystemMediaTransportControlsSessionPlaybackInfo";
}
unsafe impl Send for GlobalSystemMediaTransportControlsSessionPlaybackInfo {}
unsafe impl Sync for GlobalSystemMediaTransportControlsSessionPlaybackInfo {}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GlobalSystemMediaTransportControlsSessionPlaybackStatus(pub i32);
impl GlobalSystemMediaTransportControlsSessionPlaybackStatus {
    pub const Closed: Self = Self(0);
    pub const Opened: Self = Self(1);
    pub const Changing: Self = Self(2);
    pub const Stopped: Self = Self(3);
    pub const Playing: Self = Self(4);
    pub const Paused: Self = Self(5);
}
impl windows_core::imp::TypeKind for GlobalSystemMediaTransportControlsSessionPlaybackStatus {
    type TypeKind = windows_core::imp::CopyType;
}
impl windows_core::RuntimeType for GlobalSystemMediaTransportControlsSessionPlaybackStatus {
    const SIGNATURE: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"enum(Windows.Media.Control.GlobalSystemMediaTransportControlsSessionPlaybackStatus;i4)",
    );
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.Media.Control.GlobalSystemMediaTransportControlsSessionPlaybackStatus",
    );
}
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalSystemMediaTransportControlsSessionTimelineProperties(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    GlobalSystemMediaTransportControlsSessionTimelineProperties,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl GlobalSystemMediaTransportControlsSessionTimelineProperties {
    pub fn StartTime(&self) -> windows_core::Result<windows_time::TimeSpan> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).StartTime)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn EndTime(&self) -> windows_core::Result<windows_time::TimeSpan> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).EndTime)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn MinSeekTime(&self) -> windows_core::Result<windows_time::TimeSpan> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).MinSeekTime)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn MaxSeekTime(&self) -> windows_core::Result<windows_time::TimeSpan> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).MaxSeekTime)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn Position(&self) -> windows_core::Result<windows_time::TimeSpan> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).Position)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn LastUpdatedTime(&self) -> windows_core::Result<windows_time::DateTime> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).LastUpdatedTime)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
}
impl windows_core::RuntimeType for GlobalSystemMediaTransportControlsSessionTimelineProperties {
    const SIGNATURE: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::for_class::<
        Self,
        IGlobalSystemMediaTransportControlsSessionTimelineProperties,
    >();
}
unsafe impl windows_core::Interface
    for GlobalSystemMediaTransportControlsSessionTimelineProperties
{
    type Vtable = < IGlobalSystemMediaTransportControlsSessionTimelineProperties as windows_core::Interface >::Vtable ;
    const IID : windows_core::GUID = < IGlobalSystemMediaTransportControlsSessionTimelineProperties as windows_core::Interface >::IID ;
}
impl windows_core::RuntimeName for GlobalSystemMediaTransportControlsSessionTimelineProperties {
    const NAME: &'static str =
        "Windows.Media.Control.GlobalSystemMediaTransportControlsSessionTimelineProperties";
}
unsafe impl Send for GlobalSystemMediaTransportControlsSessionTimelineProperties {}
unsafe impl Sync for GlobalSystemMediaTransportControlsSessionTimelineProperties {}
windows_core::imp::define_interface!(
    IAudioEndpointVolume,
    IAudioEndpointVolume_Vtbl,
    0x5cdf2c82_841e_4546_9722_0cf74078229a
);
windows_core::imp::interface_hierarchy!(IAudioEndpointVolume, windows_core::IUnknown);
impl IAudioEndpointVolume {
    pub unsafe fn GetChannelCount(&self) -> windows_core::Result<u32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetChannelCount)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn SetMasterVolumeLevel(
        &self,
        fleveldb: f32,
        pguideventcontext: *const windows_core::GUID,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetMasterVolumeLevel)(
                windows_core::Interface::as_raw(self),
                fleveldb,
                pguideventcontext,
            )
        }
    }
    pub unsafe fn SetMasterVolumeLevelScalar(
        &self,
        flevel: f32,
        pguideventcontext: *const windows_core::GUID,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetMasterVolumeLevelScalar)(
                windows_core::Interface::as_raw(self),
                flevel,
                pguideventcontext,
            )
        }
    }
    pub unsafe fn GetMasterVolumeLevel(&self) -> windows_core::Result<f32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetMasterVolumeLevel)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn GetMasterVolumeLevelScalar(&self) -> windows_core::Result<f32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetMasterVolumeLevelScalar)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn SetChannelVolumeLevel(
        &self,
        nchannel: u32,
        fleveldb: f32,
        pguideventcontext: *const windows_core::GUID,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetChannelVolumeLevel)(
                windows_core::Interface::as_raw(self),
                nchannel,
                fleveldb,
                pguideventcontext,
            )
        }
    }
    pub unsafe fn SetChannelVolumeLevelScalar(
        &self,
        nchannel: u32,
        flevel: f32,
        pguideventcontext: *const windows_core::GUID,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetChannelVolumeLevelScalar)(
                windows_core::Interface::as_raw(self),
                nchannel,
                flevel,
                pguideventcontext,
            )
        }
    }
    pub unsafe fn GetChannelVolumeLevel(&self, nchannel: u32) -> windows_core::Result<f32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetChannelVolumeLevel)(
                windows_core::Interface::as_raw(self),
                nchannel,
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn GetChannelVolumeLevelScalar(&self, nchannel: u32) -> windows_core::Result<f32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetChannelVolumeLevelScalar)(
                windows_core::Interface::as_raw(self),
                nchannel,
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn SetMute(
        &self,
        bmute: bool,
        pguideventcontext: *const windows_core::GUID,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetMute)(
                windows_core::Interface::as_raw(self),
                bmute.into(),
                pguideventcontext,
            )
        }
    }
    pub unsafe fn GetMute(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetMute)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn GetVolumeStepInfo(
        &self,
        pnstep: *mut u32,
        pnstepcount: *mut u32,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).GetVolumeStepInfo)(
                windows_core::Interface::as_raw(self),
                pnstep as _,
                pnstepcount as _,
            )
        }
    }
    pub unsafe fn VolumeStepUp(
        &self,
        pguideventcontext: *const windows_core::GUID,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).VolumeStepUp)(
                windows_core::Interface::as_raw(self),
                pguideventcontext,
            )
        }
    }
    pub unsafe fn VolumeStepDown(
        &self,
        pguideventcontext: *const windows_core::GUID,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).VolumeStepDown)(
                windows_core::Interface::as_raw(self),
                pguideventcontext,
            )
        }
    }
    pub unsafe fn QueryHardwareSupport(&self) -> windows_core::Result<u32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).QueryHardwareSupport)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn GetVolumeRange(
        &self,
        pflvolumemindb: *mut f32,
        pflvolumemaxdb: *mut f32,
        pflvolumeincrementdb: *mut f32,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).GetVolumeRange)(
                windows_core::Interface::as_raw(self),
                pflvolumemindb as _,
                pflvolumemaxdb as _,
                pflvolumeincrementdb as _,
            )
        }
    }
}
#[repr(C)]
pub struct IAudioEndpointVolume_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    RegisterControlChangeNotify: usize,
    UnregisterControlChangeNotify: usize,
    pub GetChannelCount:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut u32) -> windows_core::HRESULT,
    pub SetMasterVolumeLevel: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        f32,
        *const windows_core::GUID,
    ) -> windows_core::HRESULT,
    pub SetMasterVolumeLevelScalar: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        f32,
        *const windows_core::GUID,
    ) -> windows_core::HRESULT,
    pub GetMasterVolumeLevel:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut f32) -> windows_core::HRESULT,
    pub GetMasterVolumeLevelScalar:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut f32) -> windows_core::HRESULT,
    pub SetChannelVolumeLevel: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        u32,
        f32,
        *const windows_core::GUID,
    ) -> windows_core::HRESULT,
    pub SetChannelVolumeLevelScalar: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        u32,
        f32,
        *const windows_core::GUID,
    ) -> windows_core::HRESULT,
    pub GetChannelVolumeLevel:
        unsafe extern "system" fn(*mut core::ffi::c_void, u32, *mut f32) -> windows_core::HRESULT,
    pub GetChannelVolumeLevelScalar:
        unsafe extern "system" fn(*mut core::ffi::c_void, u32, *mut f32) -> windows_core::HRESULT,
    pub SetMute: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::BOOL,
        *const windows_core::GUID,
    ) -> windows_core::HRESULT,
    pub GetMute: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub GetVolumeStepInfo: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut u32,
        *mut u32,
    ) -> windows_core::HRESULT,
    pub VolumeStepUp: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const windows_core::GUID,
    ) -> windows_core::HRESULT,
    pub VolumeStepDown: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const windows_core::GUID,
    ) -> windows_core::HRESULT,
    pub QueryHardwareSupport:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut u32) -> windows_core::HRESULT,
    pub GetVolumeRange: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut f32,
        *mut f32,
        *mut f32,
    ) -> windows_core::HRESULT,
}
impl windows_core::RuntimeName for IAudioEndpointVolume {}
windows_core::imp::define_interface!(
    ICurrentSessionChangedEventArgs,
    ICurrentSessionChangedEventArgs_Vtbl,
    0x6969cb39_0bfa_5fe0_8d73_09cc5e5408e1
);
impl windows_core::RuntimeType for ICurrentSessionChangedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.Media.Control.ICurrentSessionChangedEventArgs",
    );
}
#[repr(C)]
pub struct ICurrentSessionChangedEventArgs_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
}
windows_core::imp::define_interface!(
    IDispatch,
    IDispatch_Vtbl,
    0x00020400_0000_0000_c000_000000000046
);
windows_core::imp::interface_hierarchy!(IDispatch, windows_core::IUnknown);
impl IDispatch {
    pub unsafe fn GetTypeInfoCount(&self) -> windows_core::Result<u32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetTypeInfoCount)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
}
#[repr(C)]
pub struct IDispatch_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub GetTypeInfoCount:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut u32) -> windows_core::HRESULT,
    GetTypeInfo: usize,
    GetIDsOfNames: usize,
    Invoke: usize,
}
impl windows_core::RuntimeName for IDispatch {}
windows_core::imp::define_interface!(
    IGlobalSystemMediaTransportControlsSession,
    IGlobalSystemMediaTransportControlsSession_Vtbl,
    0x7148c835_9b14_5ae2_ab85_dc9b1c14e1a8
);
impl windows_core::RuntimeType for IGlobalSystemMediaTransportControlsSession {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.Media.Control.IGlobalSystemMediaTransportControlsSession",
    );
}
#[repr(C)]
pub struct IGlobalSystemMediaTransportControlsSession_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub SourceAppUserModelId: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub TryGetMediaPropertiesAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetTimelineProperties: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetPlaybackInfo: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub TryPlayAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub TryPauseAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub TryStopAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub TryRecordAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub TryFastForwardAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub TryRewindAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub TrySkipNextAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub TrySkipPreviousAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub TryChangeChannelUpAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub TryChangeChannelDownAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub TryTogglePlayPauseAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub TryChangeAutoRepeatModeAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        MediaPlaybackAutoRepeatMode,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub TryChangePlaybackRateAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        f64,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub TryChangeShuffleActiveAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        bool,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub TryChangePlaybackPositionAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        i64,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub TimelinePropertiesChanged: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut i64,
    ) -> windows_core::HRESULT,
    pub RemoveTimelinePropertiesChanged:
        unsafe extern "system" fn(*mut core::ffi::c_void, i64) -> windows_core::HRESULT,
    pub PlaybackInfoChanged: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut i64,
    ) -> windows_core::HRESULT,
    pub RemovePlaybackInfoChanged:
        unsafe extern "system" fn(*mut core::ffi::c_void, i64) -> windows_core::HRESULT,
    pub MediaPropertiesChanged: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut i64,
    ) -> windows_core::HRESULT,
    pub RemoveMediaPropertiesChanged:
        unsafe extern "system" fn(*mut core::ffi::c_void, i64) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IGlobalSystemMediaTransportControlsSessionManager,
    IGlobalSystemMediaTransportControlsSessionManager_Vtbl,
    0xcace8eac_e86e_504a_ab31_5ff8ff1bce49
);
impl windows_core::RuntimeType for IGlobalSystemMediaTransportControlsSessionManager {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.Media.Control.IGlobalSystemMediaTransportControlsSessionManager",
    );
}
#[repr(C)]
pub struct IGlobalSystemMediaTransportControlsSessionManager_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub GetCurrentSession: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetSessions: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CurrentSessionChanged: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut i64,
    ) -> windows_core::HRESULT,
    pub RemoveCurrentSessionChanged:
        unsafe extern "system" fn(*mut core::ffi::c_void, i64) -> windows_core::HRESULT,
    pub SessionsChanged: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut i64,
    ) -> windows_core::HRESULT,
    pub RemoveSessionsChanged:
        unsafe extern "system" fn(*mut core::ffi::c_void, i64) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IGlobalSystemMediaTransportControlsSessionManagerStatics,
    IGlobalSystemMediaTransportControlsSessionManagerStatics_Vtbl,
    0x2050c4ee_11a0_57de_aed7_c97c70338245
);
impl windows_core::RuntimeType for IGlobalSystemMediaTransportControlsSessionManagerStatics {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.Media.Control.IGlobalSystemMediaTransportControlsSessionManagerStatics",
    );
}
#[repr(C)]
pub struct IGlobalSystemMediaTransportControlsSessionManagerStatics_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub RequestAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IGlobalSystemMediaTransportControlsSessionMediaProperties,
    IGlobalSystemMediaTransportControlsSessionMediaProperties_Vtbl,
    0x68856cf6_adb4_54b2_ac16_05837907acb6
);
impl windows_core::RuntimeType for IGlobalSystemMediaTransportControlsSessionMediaProperties {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.Media.Control.IGlobalSystemMediaTransportControlsSessionMediaProperties",
    );
}
#[repr(C)]
pub struct IGlobalSystemMediaTransportControlsSessionMediaProperties_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub Title: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub Subtitle: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub AlbumArtist: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub Artist: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub AlbumTitle: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub TrackNumber:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut i32) -> windows_core::HRESULT,
    pub Genres: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub AlbumTrackCount:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut i32) -> windows_core::HRESULT,
    pub PlaybackType: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IGlobalSystemMediaTransportControlsSessionPlaybackControls,
    IGlobalSystemMediaTransportControlsSessionPlaybackControls_Vtbl,
    0x6501a3e6_bc7a_503a_bb1b_68f158f3fb03
);
impl windows_core::RuntimeType for IGlobalSystemMediaTransportControlsSessionPlaybackControls {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.Media.Control.IGlobalSystemMediaTransportControlsSessionPlaybackControls",
    );
}
#[repr(C)]
pub struct IGlobalSystemMediaTransportControlsSessionPlaybackControls_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub IsPlayEnabled:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
    pub IsPauseEnabled:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
    pub IsStopEnabled:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
    pub IsRecordEnabled:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
    pub IsFastForwardEnabled:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
    pub IsRewindEnabled:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
    pub IsNextEnabled:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
    pub IsPreviousEnabled:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
    pub IsChannelUpEnabled:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
    pub IsChannelDownEnabled:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
    pub IsPlayPauseToggleEnabled:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
    pub IsShuffleEnabled:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
    pub IsRepeatEnabled:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
    pub IsPlaybackRateEnabled:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
    pub IsPlaybackPositionEnabled:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IGlobalSystemMediaTransportControlsSessionPlaybackInfo,
    IGlobalSystemMediaTransportControlsSessionPlaybackInfo_Vtbl,
    0x94b4b6cf_e8ba_51ad_87a7_c10ade106127
);
impl windows_core::RuntimeType for IGlobalSystemMediaTransportControlsSessionPlaybackInfo {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.Media.Control.IGlobalSystemMediaTransportControlsSessionPlaybackInfo",
    );
}
#[repr(C)]
pub struct IGlobalSystemMediaTransportControlsSessionPlaybackInfo_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub Controls: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub PlaybackStatus: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut GlobalSystemMediaTransportControlsSessionPlaybackStatus,
    ) -> windows_core::HRESULT,
    pub PlaybackType: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub AutoRepeatMode: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub PlaybackRate: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub IsShuffleActive: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IGlobalSystemMediaTransportControlsSessionTimelineProperties,
    IGlobalSystemMediaTransportControlsSessionTimelineProperties_Vtbl,
    0xede34136_6f25_588d_8ecf_ea5b6735aaa5
);
impl windows_core::RuntimeType for IGlobalSystemMediaTransportControlsSessionTimelineProperties {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.Media.Control.IGlobalSystemMediaTransportControlsSessionTimelineProperties",
    );
}
#[repr(C)]
pub struct IGlobalSystemMediaTransportControlsSessionTimelineProperties_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub StartTime: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_time::TimeSpan,
    ) -> windows_core::HRESULT,
    pub EndTime: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_time::TimeSpan,
    ) -> windows_core::HRESULT,
    pub MinSeekTime: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_time::TimeSpan,
    ) -> windows_core::HRESULT,
    pub MaxSeekTime: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_time::TimeSpan,
    ) -> windows_core::HRESULT,
    pub Position: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_time::TimeSpan,
    ) -> windows_core::HRESULT,
    pub LastUpdatedTime: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_time::DateTime,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IMMDevice,
    IMMDevice_Vtbl,
    0xd666063f_1587_4e43_81f1_b948e807363f
);
windows_core::imp::interface_hierarchy!(IMMDevice, windows_core::IUnknown);
impl IMMDevice {
    pub unsafe fn Activate<T>(
        &self,
        dwclsctx: u32,
        pactivationparams: Option<*const PROPVARIANT>,
    ) -> windows_core::Result<T>
    where
        T: windows_core::Interface,
    {
        let mut result__ = core::ptr::null_mut();
        unsafe {
            (windows_core::Interface::vtable(self).Activate)(
                windows_core::Interface::as_raw(self),
                &T::IID,
                dwclsctx,
                pactivationparams.unwrap_or(core::mem::zeroed()) as _,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn GetId(&self) -> windows_core::Result<windows_core::PWSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetId)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn GetState(&self) -> windows_core::Result<u32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetState)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
}
#[repr(C)]
pub struct IMMDevice_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub Activate: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const windows_core::GUID,
        u32,
        *const PROPVARIANT,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    OpenPropertyStore: usize,
    pub GetId: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::PWSTR,
    ) -> windows_core::HRESULT,
    pub GetState:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut u32) -> windows_core::HRESULT,
}
impl windows_core::RuntimeName for IMMDevice {}
windows_core::imp::define_interface!(
    IMMDeviceEnumerator,
    IMMDeviceEnumerator_Vtbl,
    0xa95664d2_9614_4f35_a746_de8db63617e6
);
windows_core::imp::interface_hierarchy!(IMMDeviceEnumerator, windows_core::IUnknown);
impl IMMDeviceEnumerator {
    pub unsafe fn GetDefaultAudioEndpoint(
        &self,
        dataflow: EDataFlow,
        role: ERole,
    ) -> windows_core::Result<IMMDevice> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetDefaultAudioEndpoint)(
                windows_core::Interface::as_raw(self),
                dataflow,
                role,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn GetDevice<P0>(&self, pwstrid: P0) -> windows_core::Result<IMMDevice>
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetDevice)(
                windows_core::Interface::as_raw(self),
                pwstrid.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
}
#[repr(C)]
pub struct IMMDeviceEnumerator_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    EnumAudioEndpoints: usize,
    pub GetDefaultAudioEndpoint: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        EDataFlow,
        ERole,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetDevice: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    RegisterEndpointNotificationCallback: usize,
    UnregisterEndpointNotificationCallback: usize,
}
impl windows_core::RuntimeName for IMMDeviceEnumerator {}
windows_core::imp::define_interface!(
    IMediaPropertiesChangedEventArgs,
    IMediaPropertiesChangedEventArgs_Vtbl,
    0x7d3741cb_adf0_5cef_91ba_cfabcdd77678
);
impl windows_core::RuntimeType for IMediaPropertiesChangedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.Media.Control.IMediaPropertiesChangedEventArgs",
    );
}
#[repr(C)]
pub struct IMediaPropertiesChangedEventArgs_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
}
windows_core::imp::define_interface!(
    IPlaybackInfoChangedEventArgs,
    IPlaybackInfoChangedEventArgs_Vtbl,
    0x786756c2_bc0d_50a5_8807_054291fef139
);
impl windows_core::RuntimeType for IPlaybackInfoChangedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.Media.Control.IPlaybackInfoChangedEventArgs",
    );
}
#[repr(C)]
pub struct IPlaybackInfoChangedEventArgs_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
}
windows_core::imp::define_interface!(
    IPropertySet,
    IPropertySet_Vtbl,
    0x8a43ed9f_f4e6_4421_acf9_1dab2986820c
);
impl windows_core::RuntimeType for IPropertySet {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"Windows.Foundation.Collections.IPropertySet");
}
windows_core::imp::interface_hierarchy!(
    IPropertySet,
    windows_core::IUnknown,
    windows_core::IInspectable
);
windows_core::imp::required_hierarchy!(IPropertySet, windows_collections::IIterable < windows_collections::IKeyValuePair < windows_core::HSTRING, windows_core::IInspectable > >, windows_collections::IMap < windows_core::HSTRING, windows_core::IInspectable >, windows_collections::IObservableMap < windows_core::HSTRING, windows_core::IInspectable >);
impl IPropertySet {
    pub fn First(
        &self,
    ) -> windows_core::Result<
        windows_collections::IIterator<
            windows_collections::IKeyValuePair<windows_core::HSTRING, windows_core::IInspectable>,
        >,
    > {
        let this = &windows_core::Interface::cast::<
            windows_collections::IIterable<
                windows_collections::IKeyValuePair<
                    windows_core::HSTRING,
                    windows_core::IInspectable,
                >,
            >,
        >(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).First)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn Lookup(
        &self,
        key: &windows_core::HSTRING,
    ) -> windows_core::Result<windows_core::IInspectable> {
        let this = &windows_core::Interface::cast::<
            windows_collections::IMap<windows_core::HSTRING, windows_core::IInspectable>,
        >(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).Lookup)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(key),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn Size(&self) -> windows_core::Result<u32> {
        let this = &windows_core::Interface::cast::<
            windows_collections::IMap<windows_core::HSTRING, windows_core::IInspectable>,
        >(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).Size)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn HasKey(&self, key: &windows_core::HSTRING) -> windows_core::Result<bool> {
        let this = &windows_core::Interface::cast::<
            windows_collections::IMap<windows_core::HSTRING, windows_core::IInspectable>,
        >(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).HasKey)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(key),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn GetView(
        &self,
    ) -> windows_core::Result<
        windows_collections::IMapView<windows_core::HSTRING, windows_core::IInspectable>,
    > {
        let this = &windows_core::Interface::cast::<
            windows_collections::IMap<windows_core::HSTRING, windows_core::IInspectable>,
        >(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).GetView)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn Insert<P1>(&self, key: &windows_core::HSTRING, value: P1) -> windows_core::Result<bool>
    where
        P1: windows_core::Param<windows_core::IInspectable>,
    {
        let this = &windows_core::Interface::cast::<
            windows_collections::IMap<windows_core::HSTRING, windows_core::IInspectable>,
        >(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).Insert)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(key),
                value.param().abi(),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn Remove(&self, key: &windows_core::HSTRING) -> windows_core::Result<()> {
        let this = &windows_core::Interface::cast::<
            windows_collections::IMap<windows_core::HSTRING, windows_core::IInspectable>,
        >(self)?;
        unsafe {
            (windows_core::Interface::vtable(this).Remove)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(key),
            )
            .ok()
        }
    }
    pub fn Clear(&self) -> windows_core::Result<()> {
        let this = &windows_core::Interface::cast::<
            windows_collections::IMap<windows_core::HSTRING, windows_core::IInspectable>,
        >(self)?;
        unsafe {
            (windows_core::Interface::vtable(this).Clear)(windows_core::Interface::as_raw(this))
                .ok()
        }
    }
    pub fn MapChanged<F>(&self, handler: F) -> windows_core::Result<windows_core::EventRevoker>
    where
        F: Fn(
                windows_core::Ref<
                    windows_collections::IObservableMap<
                        windows_core::HSTRING,
                        windows_core::IInspectable,
                    >,
                >,
                windows_core::Ref<windows_collections::IMapChangedEventArgs<windows_core::HSTRING>>,
            ) + Send
            + 'static,
    {
        let this = &windows_core::Interface::cast::<
            windows_collections::IObservableMap<windows_core::HSTRING, windows_core::IInspectable>,
        >(self)?;
        let handler = <windows_collections::MapChangedEventHandler<
            windows_core::HSTRING,
            windows_core::IInspectable,
        >>::new(move |a0, a1| {
            handler(a0, a1);
            Ok(())
        });
        unsafe {
            let mut result__ = core::mem::zeroed();
            let token__ = (windows_core::Interface::vtable(this).MapChanged)(
                windows_core::Interface::as_raw(this),
                windows_core::Interface::as_raw(&handler),
                &mut result__,
            )
            .map(|| result__)?;
            Ok(windows_core::EventRevoker::new(
                this.clone(),
                token__,
                windows_core::Interface::vtable(this).RemoveMapChanged,
            ))
        }
    }
}
impl IntoIterator for IPropertySet {
    type Item =
        windows_collections::IKeyValuePair<windows_core::HSTRING, windows_core::IInspectable>;
    type IntoIter = windows_collections::BufferedIterator<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        IntoIterator::into_iter(&self)
    }
}
impl IntoIterator for &IPropertySet {
    type Item =
        windows_collections::IKeyValuePair<windows_core::HSTRING, windows_core::IInspectable>;
    type IntoIter = windows_collections::BufferedIterator<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        windows_collections::BufferedIterator::new(self.First().unwrap())
    }
}
impl windows_core::RuntimeName for IPropertySet {
    const NAME: &'static str = "Windows.Foundation.Collections.IPropertySet";
}
pub trait IPropertySet_Impl:
    windows_collections::IIterable_Impl<
        windows_collections::IKeyValuePair<windows_core::HSTRING, windows_core::IInspectable>,
    > + windows_collections::IMap_Impl<windows_core::HSTRING, windows_core::IInspectable>
    + windows_collections::IObservableMap_Impl<windows_core::HSTRING, windows_core::IInspectable>
{
}
impl IPropertySet_Vtbl {
    pub const fn new<Identity: IPropertySet_Impl, const OFFSET: isize>() -> Self {
        Self {
            base__: windows_core::IInspectable_Vtbl::new::<Identity, IPropertySet, OFFSET>(),
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<IPropertySet as windows_core::Interface>::IID
    }
}
#[repr(C)]
pub struct IPropertySet_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
}
windows_core::imp::define_interface!(
    IPropertyValue,
    IPropertyValue_Vtbl,
    0x4bd682dd_7554_40e9_9a9b_82654ede7e62
);
impl windows_core::RuntimeType for IPropertyValue {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"Windows.Foundation.IPropertyValue");
}
windows_core::imp::interface_hierarchy!(
    IPropertyValue,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl IPropertyValue {
    pub fn IsNumericScalar(&self) -> windows_core::Result<bool> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).IsNumericScalar)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn GetUInt8(&self) -> windows_core::Result<u8> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetUInt8)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn GetInt16(&self) -> windows_core::Result<i16> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetInt16)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn GetUInt16(&self) -> windows_core::Result<u16> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetUInt16)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn GetInt32(&self) -> windows_core::Result<i32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetInt32)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn GetUInt32(&self) -> windows_core::Result<u32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetUInt32)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn GetInt64(&self) -> windows_core::Result<i64> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetInt64)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn GetUInt64(&self) -> windows_core::Result<u64> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetUInt64)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn GetSingle(&self) -> windows_core::Result<f32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetSingle)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn GetDouble(&self) -> windows_core::Result<f64> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetDouble)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn GetChar16(&self) -> windows_core::Result<u16> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetChar16)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn GetBoolean(&self) -> windows_core::Result<bool> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetBoolean)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn GetString(&self) -> windows_core::Result<windows_core::HSTRING> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetString)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn GetGuid(&self) -> windows_core::Result<windows_core::GUID> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetGuid)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn GetDateTime(&self) -> windows_core::Result<windows_time::DateTime> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetDateTime)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn GetTimeSpan(&self) -> windows_core::Result<windows_time::TimeSpan> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetTimeSpan)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn GetUInt8Array(&self, value: &mut windows_core::Array<u8>) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).GetUInt8Array)(
                windows_core::Interface::as_raw(self),
                value.set_abi_len(),
                value as *mut _ as _,
            )
            .ok()
        }
    }
    pub fn GetInt16Array(&self, value: &mut windows_core::Array<i16>) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).GetInt16Array)(
                windows_core::Interface::as_raw(self),
                value.set_abi_len(),
                value as *mut _ as _,
            )
            .ok()
        }
    }
    pub fn GetUInt16Array(&self, value: &mut windows_core::Array<u16>) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).GetUInt16Array)(
                windows_core::Interface::as_raw(self),
                value.set_abi_len(),
                value as *mut _ as _,
            )
            .ok()
        }
    }
    pub fn GetInt32Array(&self, value: &mut windows_core::Array<i32>) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).GetInt32Array)(
                windows_core::Interface::as_raw(self),
                value.set_abi_len(),
                value as *mut _ as _,
            )
            .ok()
        }
    }
    pub fn GetUInt32Array(&self, value: &mut windows_core::Array<u32>) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).GetUInt32Array)(
                windows_core::Interface::as_raw(self),
                value.set_abi_len(),
                value as *mut _ as _,
            )
            .ok()
        }
    }
    pub fn GetInt64Array(&self, value: &mut windows_core::Array<i64>) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).GetInt64Array)(
                windows_core::Interface::as_raw(self),
                value.set_abi_len(),
                value as *mut _ as _,
            )
            .ok()
        }
    }
    pub fn GetUInt64Array(&self, value: &mut windows_core::Array<u64>) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).GetUInt64Array)(
                windows_core::Interface::as_raw(self),
                value.set_abi_len(),
                value as *mut _ as _,
            )
            .ok()
        }
    }
    pub fn GetSingleArray(&self, value: &mut windows_core::Array<f32>) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).GetSingleArray)(
                windows_core::Interface::as_raw(self),
                value.set_abi_len(),
                value as *mut _ as _,
            )
            .ok()
        }
    }
    pub fn GetDoubleArray(&self, value: &mut windows_core::Array<f64>) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).GetDoubleArray)(
                windows_core::Interface::as_raw(self),
                value.set_abi_len(),
                value as *mut _ as _,
            )
            .ok()
        }
    }
    pub fn GetChar16Array(&self, value: &mut windows_core::Array<u16>) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).GetChar16Array)(
                windows_core::Interface::as_raw(self),
                value.set_abi_len(),
                value as *mut _ as _,
            )
            .ok()
        }
    }
    pub fn GetBooleanArray(
        &self,
        value: &mut windows_core::Array<bool>,
    ) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).GetBooleanArray)(
                windows_core::Interface::as_raw(self),
                value.set_abi_len(),
                value as *mut _ as _,
            )
            .ok()
        }
    }
    pub fn GetStringArray(
        &self,
        value: &mut windows_core::Array<windows_core::HSTRING>,
    ) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).GetStringArray)(
                windows_core::Interface::as_raw(self),
                value.set_abi_len(),
                value as *mut _ as _,
            )
            .ok()
        }
    }
    pub fn GetInspectableArray(
        &self,
        value: &mut windows_core::Array<windows_core::IInspectable>,
    ) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).GetInspectableArray)(
                windows_core::Interface::as_raw(self),
                value.set_abi_len(),
                value as *mut _ as _,
            )
            .ok()
        }
    }
    pub fn GetGuidArray(
        &self,
        value: &mut windows_core::Array<windows_core::GUID>,
    ) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).GetGuidArray)(
                windows_core::Interface::as_raw(self),
                value.set_abi_len(),
                value as *mut _ as _,
            )
            .ok()
        }
    }
    pub fn GetDateTimeArray(
        &self,
        value: &mut windows_core::Array<windows_time::DateTime>,
    ) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).GetDateTimeArray)(
                windows_core::Interface::as_raw(self),
                value.set_abi_len(),
                value as *mut _ as _,
            )
            .ok()
        }
    }
    pub fn GetTimeSpanArray(
        &self,
        value: &mut windows_core::Array<windows_time::TimeSpan>,
    ) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).GetTimeSpanArray)(
                windows_core::Interface::as_raw(self),
                value.set_abi_len(),
                value as *mut _ as _,
            )
            .ok()
        }
    }
}
impl windows_core::RuntimeName for IPropertyValue {
    const NAME: &'static str = "Windows.Foundation.IPropertyValue";
}
#[repr(C)]
pub struct IPropertyValue_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    Type: usize,
    pub IsNumericScalar:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
    pub GetUInt8:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut u8) -> windows_core::HRESULT,
    pub GetInt16:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut i16) -> windows_core::HRESULT,
    pub GetUInt16:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut u16) -> windows_core::HRESULT,
    pub GetInt32:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut i32) -> windows_core::HRESULT,
    pub GetUInt32:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut u32) -> windows_core::HRESULT,
    pub GetInt64:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut i64) -> windows_core::HRESULT,
    pub GetUInt64:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut u64) -> windows_core::HRESULT,
    pub GetSingle:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut f32) -> windows_core::HRESULT,
    pub GetDouble:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut f64) -> windows_core::HRESULT,
    pub GetChar16:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut u16) -> windows_core::HRESULT,
    pub GetBoolean:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
    pub GetString: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetGuid: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::GUID,
    ) -> windows_core::HRESULT,
    pub GetDateTime: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_time::DateTime,
    ) -> windows_core::HRESULT,
    pub GetTimeSpan: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_time::TimeSpan,
    ) -> windows_core::HRESULT,
    GetPoint: usize,
    GetSize: usize,
    GetRect: usize,
    pub GetUInt8Array: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut u32,
        *mut *mut u8,
    ) -> windows_core::HRESULT,
    pub GetInt16Array: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut u32,
        *mut *mut i16,
    ) -> windows_core::HRESULT,
    pub GetUInt16Array: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut u32,
        *mut *mut u16,
    ) -> windows_core::HRESULT,
    pub GetInt32Array: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut u32,
        *mut *mut i32,
    ) -> windows_core::HRESULT,
    pub GetUInt32Array: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut u32,
        *mut *mut u32,
    ) -> windows_core::HRESULT,
    pub GetInt64Array: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut u32,
        *mut *mut i64,
    ) -> windows_core::HRESULT,
    pub GetUInt64Array: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut u32,
        *mut *mut u64,
    ) -> windows_core::HRESULT,
    pub GetSingleArray: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut u32,
        *mut *mut f32,
    ) -> windows_core::HRESULT,
    pub GetDoubleArray: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut u32,
        *mut *mut f64,
    ) -> windows_core::HRESULT,
    pub GetChar16Array: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut u32,
        *mut *mut u16,
    ) -> windows_core::HRESULT,
    pub GetBooleanArray: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut u32,
        *mut *mut bool,
    ) -> windows_core::HRESULT,
    pub GetStringArray: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut u32,
        *mut *mut windows_core::HSTRING,
    ) -> windows_core::HRESULT,
    pub GetInspectableArray: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut u32,
        *mut *mut windows_core::IInspectable,
    ) -> windows_core::HRESULT,
    pub GetGuidArray: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut u32,
        *mut *mut windows_core::GUID,
    ) -> windows_core::HRESULT,
    pub GetDateTimeArray: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut u32,
        *mut *mut windows_time::DateTime,
    ) -> windows_core::HRESULT,
    pub GetTimeSpanArray: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut u32,
        *mut *mut windows_time::TimeSpan,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IRecordInfo,
    IRecordInfo_Vtbl,
    0x0000002f_0000_0000_c000_000000000046
);
windows_core::imp::interface_hierarchy!(IRecordInfo, windows_core::IUnknown);
impl IRecordInfo {
    pub unsafe fn RecordInit(&self, pvnew: *mut core::ffi::c_void) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).RecordInit)(
                windows_core::Interface::as_raw(self),
                pvnew as _,
            )
        }
    }
    pub unsafe fn RecordClear(
        &self,
        pvexisting: *const core::ffi::c_void,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).RecordClear)(
                windows_core::Interface::as_raw(self),
                pvexisting,
            )
        }
    }
    pub unsafe fn RecordCopy(
        &self,
        pvexisting: *const core::ffi::c_void,
        pvnew: *mut core::ffi::c_void,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).RecordCopy)(
                windows_core::Interface::as_raw(self),
                pvexisting,
                pvnew as _,
            )
        }
    }
    pub unsafe fn GetGuid(&self) -> windows_core::Result<windows_core::GUID> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetGuid)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn GetName(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetName)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn GetSize(&self) -> windows_core::Result<u32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetSize)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn GetField<P1>(
        &self,
        pvdata: *const core::ffi::c_void,
        szfieldname: P1,
    ) -> windows_core::Result<VARIANT>
    where
        P1: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetField)(
                windows_core::Interface::as_raw(self),
                pvdata,
                szfieldname.param().abi(),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn GetFieldNoCopy<P1>(
        &self,
        pvdata: *const core::ffi::c_void,
        szfieldname: P1,
        pvarfield: *mut VARIANT,
        ppvdatacarray: *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT
    where
        P1: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).GetFieldNoCopy)(
                windows_core::Interface::as_raw(self),
                pvdata,
                szfieldname.param().abi(),
                pvarfield,
                ppvdatacarray as _,
            )
        }
    }
    pub unsafe fn PutField<P2>(
        &self,
        wflags: u32,
        pvdata: *mut core::ffi::c_void,
        szfieldname: P2,
        pvarfield: *const VARIANT,
    ) -> windows_core::HRESULT
    where
        P2: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).PutField)(
                windows_core::Interface::as_raw(self),
                wflags,
                pvdata as _,
                szfieldname.param().abi(),
                pvarfield,
            )
        }
    }
    pub unsafe fn PutFieldNoCopy<P2>(
        &self,
        wflags: u32,
        pvdata: *mut core::ffi::c_void,
        szfieldname: P2,
        pvarfield: *const VARIANT,
    ) -> windows_core::HRESULT
    where
        P2: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).PutFieldNoCopy)(
                windows_core::Interface::as_raw(self),
                wflags,
                pvdata as _,
                szfieldname.param().abi(),
                pvarfield,
            )
        }
    }
    pub unsafe fn GetFieldNames(
        &self,
        pcnames: *mut u32,
        rgbstrnames: *mut windows_core::BSTR,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).GetFieldNames)(
                windows_core::Interface::as_raw(self),
                pcnames as _,
                core::mem::transmute(rgbstrnames),
            )
        }
    }
    pub unsafe fn IsMatchingType<P0>(&self, precordinfo: P0) -> windows_core::BOOL
    where
        P0: windows_core::Param<Self>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).IsMatchingType)(
                windows_core::Interface::as_raw(self),
                precordinfo.param().abi(),
            )
        }
    }
    pub unsafe fn RecordCreate(&self) -> *mut core::ffi::c_void {
        unsafe {
            (windows_core::Interface::vtable(self).RecordCreate)(windows_core::Interface::as_raw(
                self,
            ))
        }
    }
    pub unsafe fn RecordCreateCopy(
        &self,
        pvsource: *const core::ffi::c_void,
        ppvdest: *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).RecordCreateCopy)(
                windows_core::Interface::as_raw(self),
                pvsource,
                ppvdest as _,
            )
        }
    }
    pub unsafe fn RecordDestroy(
        &self,
        pvrecord: *const core::ffi::c_void,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).RecordDestroy)(
                windows_core::Interface::as_raw(self),
                pvrecord,
            )
        }
    }
}
#[repr(C)]
pub struct IRecordInfo_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub RecordInit: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub RecordClear: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub RecordCopy: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetGuid: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::GUID,
    ) -> windows_core::HRESULT,
    pub GetName: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetSize:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut u32) -> windows_core::HRESULT,
    GetTypeInfo: usize,
    pub GetField: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const core::ffi::c_void,
        windows_core::PCWSTR,
        *mut VARIANT,
    ) -> windows_core::HRESULT,
    pub GetFieldNoCopy: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const core::ffi::c_void,
        windows_core::PCWSTR,
        *mut VARIANT,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub PutField: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        u32,
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        *const VARIANT,
    ) -> windows_core::HRESULT,
    pub PutFieldNoCopy: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        u32,
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        *const VARIANT,
    ) -> windows_core::HRESULT,
    pub GetFieldNames: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut u32,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub IsMatchingType: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::BOOL,
    pub RecordCreate: unsafe extern "system" fn(*mut core::ffi::c_void) -> *mut core::ffi::c_void,
    pub RecordCreateCopy: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub RecordDestroy: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
impl windows_core::RuntimeName for IRecordInfo {}
windows_core::imp::define_interface!(
    ISequentialStream,
    ISequentialStream_Vtbl,
    0x0c733a30_2a1c_11ce_ade5_00aa0044773d
);
windows_core::imp::interface_hierarchy!(ISequentialStream, windows_core::IUnknown);
impl ISequentialStream {
    pub unsafe fn Read(
        &self,
        pv: *mut core::ffi::c_void,
        cb: u32,
        pcbread: Option<*mut u32>,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).Read)(
                windows_core::Interface::as_raw(self),
                pv as _,
                cb,
                pcbread.unwrap_or(core::mem::zeroed()) as _,
            )
        }
    }
    pub unsafe fn Write(
        &self,
        pv: *const core::ffi::c_void,
        cb: u32,
        pcbwritten: Option<*mut u32>,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).Write)(
                windows_core::Interface::as_raw(self),
                pv,
                cb,
                pcbwritten.unwrap_or(core::mem::zeroed()) as _,
            )
        }
    }
}
#[repr(C)]
pub struct ISequentialStream_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub Read: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        u32,
        *mut u32,
    ) -> windows_core::HRESULT,
    pub Write: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const core::ffi::c_void,
        u32,
        *mut u32,
    ) -> windows_core::HRESULT,
}
pub trait ISequentialStream_Impl: windows_core::IUnknownImpl {
    fn Read(
        &self,
        pv: *mut core::ffi::c_void,
        cb: u32,
        pcbread: *mut u32,
    ) -> windows_core::Result<()>;
    fn Write(
        &self,
        pv: *const core::ffi::c_void,
        cb: u32,
        pcbwritten: *mut u32,
    ) -> windows_core::Result<()>;
}
impl ISequentialStream_Vtbl {
    pub const fn new<Identity: ISequentialStream_Impl, const OFFSET: isize>() -> Self {
        unsafe extern "system" fn Read<Identity: ISequentialStream_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            pv: *mut core::ffi::c_void,
            cb: u32,
            pcbread: *mut u32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ISequentialStream_Impl::Read(
                    this,
                    core::mem::transmute_copy(&pv),
                    core::mem::transmute_copy(&cb),
                    core::mem::transmute_copy(&pcbread),
                )
                .into()
            }
        }
        unsafe extern "system" fn Write<Identity: ISequentialStream_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            pv: *const core::ffi::c_void,
            cb: u32,
            pcbwritten: *mut u32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ISequentialStream_Impl::Write(
                    this,
                    core::mem::transmute_copy(&pv),
                    core::mem::transmute_copy(&cb),
                    core::mem::transmute_copy(&pcbwritten),
                )
                .into()
            }
        }
        Self {
            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
            Read: Read::<Identity, OFFSET>,
            Write: Write::<Identity, OFFSET>,
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<ISequentialStream as windows_core::Interface>::IID
    }
}
impl windows_core::RuntimeName for ISequentialStream {}
windows_core::imp::define_interface!(
    ISessionsChangedEventArgs,
    ISessionsChangedEventArgs_Vtbl,
    0xbbf0cd32_42c4_5a58_b317_f34bbfbd26e0
);
impl windows_core::RuntimeType for ISessionsChangedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.Media.Control.ISessionsChangedEventArgs",
    );
}
#[repr(C)]
pub struct ISessionsChangedEventArgs_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
}
windows_core::imp::define_interface!(
    IStorage,
    IStorage_Vtbl,
    0x0000000b_0000_0000_c000_000000000046
);
windows_core::imp::interface_hierarchy!(IStorage, windows_core::IUnknown);
impl IStorage {
    pub unsafe fn CopyTo<P3>(
        &self,
        rgiidexclude: Option<&[windows_core::GUID]>,
        snbexclude: Option<*const windows_core::PCWSTR>,
        pstgdest: P3,
    ) -> windows_core::HRESULT
    where
        P3: windows_core::Param<Self>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).CopyTo)(
                windows_core::Interface::as_raw(self),
                rgiidexclude.map_or(0, |slice| slice.len().try_into().unwrap()),
                rgiidexclude.map_or(core::ptr::null(), |slice| slice.as_ptr()),
                snbexclude.unwrap_or(core::mem::zeroed()) as _,
                pstgdest.param().abi(),
            )
        }
    }
    pub unsafe fn Commit(&self, grfcommitflags: u32) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).Commit)(
                windows_core::Interface::as_raw(self),
                grfcommitflags,
            )
        }
    }
    pub unsafe fn Revert(&self) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).Revert)(windows_core::Interface::as_raw(self))
        }
    }
    pub unsafe fn SetClass(&self, clsid: *const windows_core::GUID) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetClass)(
                windows_core::Interface::as_raw(self),
                clsid,
            )
        }
    }
    pub unsafe fn SetStateBits(&self, grfstatebits: u32, grfmask: u32) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetStateBits)(
                windows_core::Interface::as_raw(self),
                grfstatebits,
                grfmask,
            )
        }
    }
}
#[repr(C)]
pub struct IStorage_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    CreateStream: usize,
    OpenStream: usize,
    CreateStorage: usize,
    OpenStorage: usize,
    pub CopyTo: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        u32,
        *const windows_core::GUID,
        *const windows_core::PCWSTR,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    MoveElementTo: usize,
    pub Commit: unsafe extern "system" fn(*mut core::ffi::c_void, u32) -> windows_core::HRESULT,
    pub Revert: unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
    EnumElements: usize,
    DestroyElement: usize,
    RenameElement: usize,
    SetElementTimes: usize,
    pub SetClass: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const windows_core::GUID,
    ) -> windows_core::HRESULT,
    pub SetStateBits:
        unsafe extern "system" fn(*mut core::ffi::c_void, u32, u32) -> windows_core::HRESULT,
    Stat: usize,
}
impl windows_core::RuntimeName for IStorage {}
windows_core::imp::define_interface!(
    IStream,
    IStream_Vtbl,
    0x0000000c_0000_0000_c000_000000000046
);
impl core::ops::Deref for IStream {
    type Target = ISequentialStream;
    fn deref(&self) -> &Self::Target {
        unsafe { core::mem::transmute(self) }
    }
}
windows_core::imp::interface_hierarchy!(IStream, windows_core::IUnknown, ISequentialStream);
impl IStream {
    pub unsafe fn Seek(
        &self,
        dlibmove: i64,
        dworigin: u32,
        plibnewposition: Option<*mut u64>,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).Seek)(
                windows_core::Interface::as_raw(self),
                dlibmove,
                dworigin,
                plibnewposition.unwrap_or(core::mem::zeroed()) as _,
            )
        }
    }
    pub unsafe fn SetSize(&self, libnewsize: u64) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetSize)(
                windows_core::Interface::as_raw(self),
                libnewsize,
            )
        }
    }
    pub unsafe fn CopyTo<P0>(
        &self,
        pstm: P0,
        cb: u64,
        pcbread: Option<*mut u64>,
        pcbwritten: Option<*mut u64>,
    ) -> windows_core::HRESULT
    where
        P0: windows_core::Param<Self>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).CopyTo)(
                windows_core::Interface::as_raw(self),
                pstm.param().abi(),
                cb,
                pcbread.unwrap_or(core::mem::zeroed()) as _,
                pcbwritten.unwrap_or(core::mem::zeroed()) as _,
            )
        }
    }
    pub unsafe fn Commit(&self, grfcommitflags: u32) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).Commit)(
                windows_core::Interface::as_raw(self),
                grfcommitflags,
            )
        }
    }
    pub unsafe fn Revert(&self) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).Revert)(windows_core::Interface::as_raw(self))
        }
    }
    pub unsafe fn LockRegion(
        &self,
        liboffset: u64,
        cb: u64,
        dwlocktype: u32,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).LockRegion)(
                windows_core::Interface::as_raw(self),
                liboffset,
                cb,
                dwlocktype,
            )
        }
    }
    pub unsafe fn UnlockRegion(
        &self,
        liboffset: u64,
        cb: u64,
        dwlocktype: u32,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).UnlockRegion)(
                windows_core::Interface::as_raw(self),
                liboffset,
                cb,
                dwlocktype,
            )
        }
    }
    pub unsafe fn Clone(&self) -> windows_core::Result<Self> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).Clone)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
}
#[repr(C)]
pub struct IStream_Vtbl {
    pub base__: ISequentialStream_Vtbl,
    pub Seek: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        i64,
        u32,
        *mut u64,
    ) -> windows_core::HRESULT,
    pub SetSize: unsafe extern "system" fn(*mut core::ffi::c_void, u64) -> windows_core::HRESULT,
    pub CopyTo: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        u64,
        *mut u64,
        *mut u64,
    ) -> windows_core::HRESULT,
    pub Commit: unsafe extern "system" fn(*mut core::ffi::c_void, u32) -> windows_core::HRESULT,
    pub Revert: unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
    pub LockRegion:
        unsafe extern "system" fn(*mut core::ffi::c_void, u64, u64, u32) -> windows_core::HRESULT,
    pub UnlockRegion:
        unsafe extern "system" fn(*mut core::ffi::c_void, u64, u64, u32) -> windows_core::HRESULT,
    Stat: usize,
    pub Clone: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
impl windows_core::RuntimeName for IStream {}
windows_core::imp::define_interface!(
    ITimelinePropertiesChangedEventArgs,
    ITimelinePropertiesChangedEventArgs_Vtbl,
    0x29033a2f_c923_5a77_bcaf_055ff415ad32
);
impl windows_core::RuntimeType for ITimelinePropertiesChangedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.Media.Control.ITimelinePropertiesChangedEventArgs",
    );
}
#[repr(C)]
pub struct ITimelinePropertiesChangedEventArgs_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
}
windows_core::imp::define_interface!(
    IToastActivatedEventArgs,
    IToastActivatedEventArgs_Vtbl,
    0xe3bf92f3_c197_436f_8265_0625824f8dac
);
impl windows_core::RuntimeType for IToastActivatedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.UI.Notifications.IToastActivatedEventArgs",
    );
}
#[repr(C)]
pub struct IToastActivatedEventArgs_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub Arguments: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IToastActivatedEventArgs2,
    IToastActivatedEventArgs2_Vtbl,
    0xab7da512_cc61_568e_81be_304ac31038fa
);
impl windows_core::RuntimeType for IToastActivatedEventArgs2 {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.UI.Notifications.IToastActivatedEventArgs2",
    );
}
#[repr(C)]
pub struct IToastActivatedEventArgs2_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub UserInput: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IToastDismissedEventArgs,
    IToastDismissedEventArgs_Vtbl,
    0x3f89d935_d9cb_4538_a0f0_ffe7659938f8
);
impl windows_core::RuntimeType for IToastDismissedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.UI.Notifications.IToastDismissedEventArgs",
    );
}
#[repr(C)]
pub struct IToastDismissedEventArgs_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
}
windows_core::imp::define_interface!(
    IToastFailedEventArgs,
    IToastFailedEventArgs_Vtbl,
    0x35176862_cfd4_44f8_ad64_f500fd896c3b
);
impl windows_core::RuntimeType for IToastFailedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.UI.Notifications.IToastFailedEventArgs",
    );
}
#[repr(C)]
pub struct IToastFailedEventArgs_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub ErrorCode: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::HRESULT,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IToastNotification,
    IToastNotification_Vtbl,
    0x997e2675_059e_4e60_8b06_1760917c8b80
);
impl windows_core::RuntimeType for IToastNotification {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"Windows.UI.Notifications.IToastNotification");
}
#[repr(C)]
pub struct IToastNotification_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub Content: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SetExpirationTime: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ExpirationTime: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub Dismissed: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut i64,
    ) -> windows_core::HRESULT,
    pub RemoveDismissed:
        unsafe extern "system" fn(*mut core::ffi::c_void, i64) -> windows_core::HRESULT,
    pub Activated: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut i64,
    ) -> windows_core::HRESULT,
    pub RemoveActivated:
        unsafe extern "system" fn(*mut core::ffi::c_void, i64) -> windows_core::HRESULT,
    pub Failed: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut i64,
    ) -> windows_core::HRESULT,
    pub RemoveFailed:
        unsafe extern "system" fn(*mut core::ffi::c_void, i64) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IToastNotification2,
    IToastNotification2_Vtbl,
    0x9dfb9fd1_143a_490e_90bf_b9fba7132de7
);
impl windows_core::RuntimeType for IToastNotification2 {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"Windows.UI.Notifications.IToastNotification2");
}
#[repr(C)]
pub struct IToastNotification2_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub SetTag: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub Tag: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SetGroup: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub Group: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SetSuppressPopup:
        unsafe extern "system" fn(*mut core::ffi::c_void, bool) -> windows_core::HRESULT,
    pub SuppressPopup:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IToastNotification3,
    IToastNotification3_Vtbl,
    0x31e8aed8_8141_4f99_bc0a_c4ed21297d77
);
impl windows_core::RuntimeType for IToastNotification3 {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"Windows.UI.Notifications.IToastNotification3");
}
#[repr(C)]
pub struct IToastNotification3_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    NotificationMirroring: usize,
    SetNotificationMirroring: usize,
    pub RemoteId: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SetRemoteId: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IToastNotification4,
    IToastNotification4_Vtbl,
    0x15154935_28ea_4727_88e9_c58680e2d118
);
impl windows_core::RuntimeType for IToastNotification4 {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"Windows.UI.Notifications.IToastNotification4");
}
#[repr(C)]
pub struct IToastNotification4_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
}
windows_core::imp::define_interface!(
    IToastNotification6,
    IToastNotification6_Vtbl,
    0x43ebfe53_89ae_5c1e_a279_3aecfe9b6f54
);
impl windows_core::RuntimeType for IToastNotification6 {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"Windows.UI.Notifications.IToastNotification6");
}
#[repr(C)]
pub struct IToastNotification6_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub ExpiresOnReboot:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
    pub SetExpiresOnReboot:
        unsafe extern "system" fn(*mut core::ffi::c_void, bool) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IToastNotificationFactory,
    IToastNotificationFactory_Vtbl,
    0x04124b20_82c6_4229_b109_fd9ed4662b53
);
impl windows_core::RuntimeType for IToastNotificationFactory {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.UI.Notifications.IToastNotificationFactory",
    );
}
#[repr(C)]
pub struct IToastNotificationFactory_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub CreateToastNotification: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IToastNotificationHistory,
    IToastNotificationHistory_Vtbl,
    0x5caddc63_01d3_4c97_986f_0533483fee14
);
impl windows_core::RuntimeType for IToastNotificationHistory {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.UI.Notifications.IToastNotificationHistory",
    );
}
#[repr(C)]
pub struct IToastNotificationHistory_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub RemoveGroup: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub RemoveGroupWithId: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub RemoveGroupedTagWithId: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub RemoveGroupedTag: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub Remove: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub Clear: unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
    pub ClearWithId: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IToastNotificationHistory2,
    IToastNotificationHistory2_Vtbl,
    0x3bc3d253_2f31_4092_9129_8ad5abf067da
);
impl windows_core::RuntimeType for IToastNotificationHistory2 {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.UI.Notifications.IToastNotificationHistory2",
    );
}
#[repr(C)]
pub struct IToastNotificationHistory2_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub GetHistory: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetHistoryWithId: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IToastNotificationManagerStatics,
    IToastNotificationManagerStatics_Vtbl,
    0x50ac103f_d235_4598_bbef_98fe4d1a3ad4
);
impl windows_core::RuntimeType for IToastNotificationManagerStatics {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.UI.Notifications.IToastNotificationManagerStatics",
    );
}
#[repr(C)]
pub struct IToastNotificationManagerStatics_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub CreateToastNotifier: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CreateToastNotifierWithId: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IToastNotificationManagerStatics2,
    IToastNotificationManagerStatics2_Vtbl,
    0x7ab93c52_0e48_4750_ba9d_1a4113981847
);
impl windows_core::RuntimeType for IToastNotificationManagerStatics2 {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.UI.Notifications.IToastNotificationManagerStatics2",
    );
}
#[repr(C)]
pub struct IToastNotificationManagerStatics2_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub History: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IToastNotificationManagerStatics4,
    IToastNotificationManagerStatics4_Vtbl,
    0x8f993fd3_e516_45fb_8130_398e93fa52c3
);
impl windows_core::RuntimeType for IToastNotificationManagerStatics4 {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.UI.Notifications.IToastNotificationManagerStatics4",
    );
}
#[repr(C)]
pub struct IToastNotificationManagerStatics4_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
}
windows_core::imp::define_interface!(
    IToastNotificationManagerStatics5,
    IToastNotificationManagerStatics5_Vtbl,
    0xd6f5f569_d40d_407c_8989_88cab42cfd14
);
impl windows_core::RuntimeType for IToastNotificationManagerStatics5 {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"Windows.UI.Notifications.IToastNotificationManagerStatics5",
    );
}
#[repr(C)]
pub struct IToastNotificationManagerStatics5_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
}
windows_core::imp::define_interface!(
    IToastNotifier,
    IToastNotifier_Vtbl,
    0x75927b93_03f3_41ec_91d3_6e5bac1b38e7
);
impl windows_core::RuntimeType for IToastNotifier {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"Windows.UI.Notifications.IToastNotifier");
}
#[repr(C)]
pub struct IToastNotifier_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub Show: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub Hide: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IToastNotifier2,
    IToastNotifier2_Vtbl,
    0x354389c6_7c01_4bd5_9c20_604340cd2b74
);
impl windows_core::RuntimeType for IToastNotifier2 {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"Windows.UI.Notifications.IToastNotifier2");
}
#[repr(C)]
pub struct IToastNotifier2_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
}
windows_core::imp::define_interface!(
    IToastNotifier3,
    IToastNotifier3_Vtbl,
    0xae75a04a_3b0c_51ad_b7e8_b08ab6052549
);
impl windows_core::RuntimeType for IToastNotifier3 {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"Windows.UI.Notifications.IToastNotifier3");
}
#[repr(C)]
pub struct IToastNotifier3_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
}
windows_core::imp::define_interface!(
    IUIAutomation,
    IUIAutomation_Vtbl,
    0x30cbe57d_d9d0_452a_ab13_7ac5ac4825ee
);
windows_core::imp::interface_hierarchy!(IUIAutomation, windows_core::IUnknown);
impl IUIAutomation {
    pub unsafe fn CompareElements<P0, P1>(
        &self,
        el1: P0,
        el2: P1,
    ) -> windows_core::Result<windows_core::BOOL>
    where
        P0: windows_core::Param<IUIAutomationElement>,
        P1: windows_core::Param<IUIAutomationElement>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CompareElements)(
                windows_core::Interface::as_raw(self),
                el1.param().abi(),
                el2.param().abi(),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CompareRuntimeIds(
        &self,
        runtimeid1: *const SAFEARRAY,
        runtimeid2: *const SAFEARRAY,
    ) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CompareRuntimeIds)(
                windows_core::Interface::as_raw(self),
                runtimeid1,
                runtimeid2,
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn GetRootElement(&self) -> windows_core::Result<IUIAutomationElement> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetRootElement)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn ElementFromHandle(
        &self,
        hwnd: UIA_HWND,
    ) -> windows_core::Result<IUIAutomationElement> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).ElementFromHandle)(
                windows_core::Interface::as_raw(self),
                hwnd,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn GetFocusedElement(&self) -> windows_core::Result<IUIAutomationElement> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetFocusedElement)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn RawViewCondition(&self) -> windows_core::Result<IUIAutomationCondition> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).RawViewCondition)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn ControlViewCondition(&self) -> windows_core::Result<IUIAutomationCondition> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).ControlViewCondition)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn ContentViewCondition(&self) -> windows_core::Result<IUIAutomationCondition> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).ContentViewCondition)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn CreateTrueCondition(&self) -> windows_core::Result<IUIAutomationCondition> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CreateTrueCondition)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn CreateFalseCondition(&self) -> windows_core::Result<IUIAutomationCondition> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CreateFalseCondition)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn CreatePropertyCondition(
        &self,
        propertyid: PROPERTYID,
        value: &VARIANT,
    ) -> windows_core::Result<IUIAutomationCondition> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CreatePropertyCondition)(
                windows_core::Interface::as_raw(self),
                propertyid,
                core::mem::transmute_copy(value),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn CreateAndCondition<P0, P1>(
        &self,
        condition1: P0,
        condition2: P1,
    ) -> windows_core::Result<IUIAutomationCondition>
    where
        P0: windows_core::Param<IUIAutomationCondition>,
        P1: windows_core::Param<IUIAutomationCondition>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CreateAndCondition)(
                windows_core::Interface::as_raw(self),
                condition1.param().abi(),
                condition2.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn CreateAndConditionFromArray(
        &self,
        conditions: *const SAFEARRAY,
    ) -> windows_core::Result<IUIAutomationCondition> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CreateAndConditionFromArray)(
                windows_core::Interface::as_raw(self),
                conditions,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn CreateAndConditionFromNativeArray(
        &self,
        conditions: *const Option<IUIAutomationCondition>,
        conditioncount: i32,
    ) -> windows_core::Result<IUIAutomationCondition> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CreateAndConditionFromNativeArray)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute(conditions),
                conditioncount,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn CreateOrCondition<P0, P1>(
        &self,
        condition1: P0,
        condition2: P1,
    ) -> windows_core::Result<IUIAutomationCondition>
    where
        P0: windows_core::Param<IUIAutomationCondition>,
        P1: windows_core::Param<IUIAutomationCondition>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CreateOrCondition)(
                windows_core::Interface::as_raw(self),
                condition1.param().abi(),
                condition2.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn CreateOrConditionFromArray(
        &self,
        conditions: *const SAFEARRAY,
    ) -> windows_core::Result<IUIAutomationCondition> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CreateOrConditionFromArray)(
                windows_core::Interface::as_raw(self),
                conditions,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn CreateOrConditionFromNativeArray(
        &self,
        conditions: *const Option<IUIAutomationCondition>,
        conditioncount: i32,
    ) -> windows_core::Result<IUIAutomationCondition> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CreateOrConditionFromNativeArray)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute(conditions),
                conditioncount,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn CreateNotCondition<P0>(
        &self,
        condition: P0,
    ) -> windows_core::Result<IUIAutomationCondition>
    where
        P0: windows_core::Param<IUIAutomationCondition>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CreateNotCondition)(
                windows_core::Interface::as_raw(self),
                condition.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn RemoveAllEventHandlers(&self) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).RemoveAllEventHandlers)(
                windows_core::Interface::as_raw(self),
            )
        }
    }
    pub unsafe fn IntNativeArrayToSafeArray(
        &self,
        array: *const i32,
        arraycount: i32,
    ) -> windows_core::Result<*mut SAFEARRAY> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).IntNativeArrayToSafeArray)(
                windows_core::Interface::as_raw(self),
                array,
                arraycount,
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn IntSafeArrayToNativeArray(
        &self,
        intarray: *const SAFEARRAY,
        array: *mut *mut i32,
    ) -> windows_core::Result<i32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).IntSafeArrayToNativeArray)(
                windows_core::Interface::as_raw(self),
                intarray,
                array as _,
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn GetPropertyProgrammaticName(
        &self,
        property: PROPERTYID,
    ) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetPropertyProgrammaticName)(
                windows_core::Interface::as_raw(self),
                property,
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn GetPatternProgrammaticName(
        &self,
        pattern: PATTERNID,
    ) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetPatternProgrammaticName)(
                windows_core::Interface::as_raw(self),
                pattern,
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn PollForPotentialSupportedPatterns<P0>(
        &self,
        pelement: P0,
        patternids: *mut *mut SAFEARRAY,
        patternnames: *mut *mut SAFEARRAY,
    ) -> windows_core::HRESULT
    where
        P0: windows_core::Param<IUIAutomationElement>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).PollForPotentialSupportedPatterns)(
                windows_core::Interface::as_raw(self),
                pelement.param().abi(),
                patternids as _,
                patternnames as _,
            )
        }
    }
    pub unsafe fn PollForPotentialSupportedProperties<P0>(
        &self,
        pelement: P0,
        propertyids: *mut *mut SAFEARRAY,
        propertynames: *mut *mut SAFEARRAY,
    ) -> windows_core::HRESULT
    where
        P0: windows_core::Param<IUIAutomationElement>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).PollForPotentialSupportedProperties)(
                windows_core::Interface::as_raw(self),
                pelement.param().abi(),
                propertyids as _,
                propertynames as _,
            )
        }
    }
    pub unsafe fn CheckNotSupported(
        &self,
        value: &VARIANT,
    ) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CheckNotSupported)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(value),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn ReservedNotSupportedValue(&self) -> windows_core::Result<windows_core::IUnknown> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).ReservedNotSupportedValue)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn ReservedMixedAttributeValue(
        &self,
    ) -> windows_core::Result<windows_core::IUnknown> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).ReservedMixedAttributeValue)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
}
#[repr(C)]
pub struct IUIAutomation_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub CompareElements: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub CompareRuntimeIds: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const SAFEARRAY,
        *const SAFEARRAY,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub GetRootElement: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ElementFromHandle: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        UIA_HWND,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    ElementFromPoint: usize,
    pub GetFocusedElement: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    GetRootElementBuildCache: usize,
    ElementFromHandleBuildCache: usize,
    ElementFromPointBuildCache: usize,
    GetFocusedElementBuildCache: usize,
    CreateTreeWalker: usize,
    ControlViewWalker: usize,
    ContentViewWalker: usize,
    RawViewWalker: usize,
    pub RawViewCondition: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ControlViewCondition: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ContentViewCondition: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    CreateCacheRequest: usize,
    pub CreateTrueCondition: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CreateFalseCondition: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CreatePropertyCondition: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        PROPERTYID,
        VARIANT,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    CreatePropertyConditionEx: usize,
    pub CreateAndCondition: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CreateAndConditionFromArray: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const SAFEARRAY,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CreateAndConditionFromNativeArray: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const *mut core::ffi::c_void,
        i32,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CreateOrCondition: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CreateOrConditionFromArray: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const SAFEARRAY,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CreateOrConditionFromNativeArray: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const *mut core::ffi::c_void,
        i32,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CreateNotCondition: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    AddAutomationEventHandler: usize,
    RemoveAutomationEventHandler: usize,
    AddPropertyChangedEventHandlerNativeArray: usize,
    AddPropertyChangedEventHandler: usize,
    RemovePropertyChangedEventHandler: usize,
    AddStructureChangedEventHandler: usize,
    RemoveStructureChangedEventHandler: usize,
    AddFocusChangedEventHandler: usize,
    RemoveFocusChangedEventHandler: usize,
    pub RemoveAllEventHandlers:
        unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
    pub IntNativeArrayToSafeArray: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const i32,
        i32,
        *mut *mut SAFEARRAY,
    ) -> windows_core::HRESULT,
    pub IntSafeArrayToNativeArray: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const SAFEARRAY,
        *mut *mut i32,
        *mut i32,
    ) -> windows_core::HRESULT,
    RectToVariant: usize,
    VariantToRect: usize,
    SafeArrayToRectNativeArray: usize,
    CreateProxyFactoryEntry: usize,
    ProxyFactoryMapping: usize,
    pub GetPropertyProgrammaticName: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        PROPERTYID,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetPatternProgrammaticName: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        PATTERNID,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub PollForPotentialSupportedPatterns: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut *mut SAFEARRAY,
        *mut *mut SAFEARRAY,
    ) -> windows_core::HRESULT,
    pub PollForPotentialSupportedProperties: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut *mut SAFEARRAY,
        *mut *mut SAFEARRAY,
    )
        -> windows_core::HRESULT,
    pub CheckNotSupported: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        VARIANT,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub ReservedNotSupportedValue: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ReservedMixedAttributeValue: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    ElementFromIAccessible: usize,
    ElementFromIAccessibleBuildCache: usize,
}
impl windows_core::RuntimeName for IUIAutomation {}
windows_core::imp::define_interface!(
    IUIAutomationCondition,
    IUIAutomationCondition_Vtbl,
    0x352ffba8_0973_437c_a61f_f64cafd81df9
);
windows_core::imp::interface_hierarchy!(IUIAutomationCondition, windows_core::IUnknown);
#[repr(C)]
pub struct IUIAutomationCondition_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
}
pub trait IUIAutomationCondition_Impl: windows_core::IUnknownImpl {}
impl IUIAutomationCondition_Vtbl {
    pub const fn new<Identity: IUIAutomationCondition_Impl, const OFFSET: isize>() -> Self {
        Self {
            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<IUIAutomationCondition as windows_core::Interface>::IID
    }
}
impl windows_core::RuntimeName for IUIAutomationCondition {}
windows_core::imp::define_interface!(
    IUIAutomationElement,
    IUIAutomationElement_Vtbl,
    0xd22108aa_8ac5_49a5_837b_37bbb3d7591e
);
windows_core::imp::interface_hierarchy!(IUIAutomationElement, windows_core::IUnknown);
impl IUIAutomationElement {
    pub unsafe fn SetFocus(&self) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetFocus)(windows_core::Interface::as_raw(self))
        }
    }
    pub unsafe fn GetRuntimeId(&self) -> windows_core::Result<*mut SAFEARRAY> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetRuntimeId)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn FindFirst<P1>(
        &self,
        scope: TreeScope,
        condition: P1,
    ) -> windows_core::Result<Self>
    where
        P1: windows_core::Param<IUIAutomationCondition>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).FindFirst)(
                windows_core::Interface::as_raw(self),
                scope,
                condition.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn GetCurrentPropertyValue(
        &self,
        propertyid: PROPERTYID,
    ) -> windows_core::Result<VARIANT> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetCurrentPropertyValue)(
                windows_core::Interface::as_raw(self),
                propertyid,
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn GetCurrentPropertyValueEx(
        &self,
        propertyid: PROPERTYID,
        ignoredefaultvalue: bool,
    ) -> windows_core::Result<VARIANT> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetCurrentPropertyValueEx)(
                windows_core::Interface::as_raw(self),
                propertyid,
                ignoredefaultvalue.into(),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn GetCachedPropertyValue(
        &self,
        propertyid: PROPERTYID,
    ) -> windows_core::Result<VARIANT> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetCachedPropertyValue)(
                windows_core::Interface::as_raw(self),
                propertyid,
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn GetCachedPropertyValueEx(
        &self,
        propertyid: PROPERTYID,
        ignoredefaultvalue: bool,
    ) -> windows_core::Result<VARIANT> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetCachedPropertyValueEx)(
                windows_core::Interface::as_raw(self),
                propertyid,
                ignoredefaultvalue.into(),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn GetCurrentPatternAs<T>(&self, patternid: PATTERNID) -> windows_core::Result<T>
    where
        T: windows_core::Interface,
    {
        let mut result__ = core::ptr::null_mut();
        unsafe {
            (windows_core::Interface::vtable(self).GetCurrentPatternAs)(
                windows_core::Interface::as_raw(self),
                patternid,
                &T::IID,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn GetCachedPatternAs<T>(&self, patternid: PATTERNID) -> windows_core::Result<T>
    where
        T: windows_core::Interface,
    {
        let mut result__ = core::ptr::null_mut();
        unsafe {
            (windows_core::Interface::vtable(self).GetCachedPatternAs)(
                windows_core::Interface::as_raw(self),
                patternid,
                &T::IID,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn GetCurrentPattern(
        &self,
        patternid: PATTERNID,
    ) -> windows_core::Result<windows_core::IUnknown> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetCurrentPattern)(
                windows_core::Interface::as_raw(self),
                patternid,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn GetCachedPattern(
        &self,
        patternid: PATTERNID,
    ) -> windows_core::Result<windows_core::IUnknown> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetCachedPattern)(
                windows_core::Interface::as_raw(self),
                patternid,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn GetCachedParent(&self) -> windows_core::Result<Self> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetCachedParent)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn CurrentProcessId(&self) -> windows_core::Result<i32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentProcessId)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CurrentLocalizedControlType(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentLocalizedControlType)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CurrentName(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentName)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CurrentAcceleratorKey(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentAcceleratorKey)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CurrentAccessKey(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentAccessKey)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CurrentHasKeyboardFocus(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentHasKeyboardFocus)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CurrentIsKeyboardFocusable(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentIsKeyboardFocusable)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CurrentIsEnabled(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentIsEnabled)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CurrentAutomationId(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentAutomationId)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CurrentClassName(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentClassName)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CurrentHelpText(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentHelpText)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CurrentCulture(&self) -> windows_core::Result<i32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentCulture)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CurrentIsControlElement(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentIsControlElement)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CurrentIsContentElement(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentIsContentElement)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CurrentIsPassword(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentIsPassword)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CurrentNativeWindowHandle(&self) -> windows_core::Result<UIA_HWND> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentNativeWindowHandle)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CurrentItemType(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentItemType)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CurrentIsOffscreen(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentIsOffscreen)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CurrentFrameworkId(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentFrameworkId)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CurrentIsRequiredForForm(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentIsRequiredForForm)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CurrentItemStatus(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentItemStatus)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CurrentLabeledBy(&self) -> windows_core::Result<Self> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentLabeledBy)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn CurrentAriaRole(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentAriaRole)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CurrentAriaProperties(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentAriaProperties)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CurrentIsDataValidForForm(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentIsDataValidForForm)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CurrentProviderDescription(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentProviderDescription)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CachedProcessId(&self) -> windows_core::Result<i32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedProcessId)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CachedLocalizedControlType(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedLocalizedControlType)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CachedName(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedName)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CachedAcceleratorKey(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedAcceleratorKey)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CachedAccessKey(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedAccessKey)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CachedHasKeyboardFocus(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedHasKeyboardFocus)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CachedIsKeyboardFocusable(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedIsKeyboardFocusable)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CachedIsEnabled(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedIsEnabled)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CachedAutomationId(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedAutomationId)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CachedClassName(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedClassName)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CachedHelpText(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedHelpText)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CachedCulture(&self) -> windows_core::Result<i32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedCulture)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CachedIsControlElement(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedIsControlElement)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CachedIsContentElement(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedIsContentElement)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CachedIsPassword(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedIsPassword)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CachedNativeWindowHandle(&self) -> windows_core::Result<UIA_HWND> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedNativeWindowHandle)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CachedItemType(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedItemType)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CachedIsOffscreen(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedIsOffscreen)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CachedFrameworkId(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedFrameworkId)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CachedIsRequiredForForm(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedIsRequiredForForm)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CachedItemStatus(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedItemStatus)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CachedLabeledBy(&self) -> windows_core::Result<Self> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedLabeledBy)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn CachedAriaRole(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedAriaRole)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CachedAriaProperties(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedAriaProperties)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn CachedIsDataValidForForm(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedIsDataValidForForm)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CachedProviderDescription(&self) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedProviderDescription)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
}
#[repr(C)]
pub struct IUIAutomationElement_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub SetFocus: unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
    pub GetRuntimeId: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut SAFEARRAY,
    ) -> windows_core::HRESULT,
    pub FindFirst: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        TreeScope,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    FindAll: usize,
    FindFirstBuildCache: usize,
    FindAllBuildCache: usize,
    BuildUpdatedCache: usize,
    pub GetCurrentPropertyValue: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        PROPERTYID,
        *mut VARIANT,
    ) -> windows_core::HRESULT,
    pub GetCurrentPropertyValueEx: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        PROPERTYID,
        windows_core::BOOL,
        *mut VARIANT,
    ) -> windows_core::HRESULT,
    pub GetCachedPropertyValue: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        PROPERTYID,
        *mut VARIANT,
    ) -> windows_core::HRESULT,
    pub GetCachedPropertyValueEx: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        PROPERTYID,
        windows_core::BOOL,
        *mut VARIANT,
    ) -> windows_core::HRESULT,
    pub GetCurrentPatternAs: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        PATTERNID,
        *const windows_core::GUID,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetCachedPatternAs: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        PATTERNID,
        *const windows_core::GUID,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetCurrentPattern: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        PATTERNID,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetCachedPattern: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        PATTERNID,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetCachedParent: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    GetCachedChildren: usize,
    pub CurrentProcessId:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut i32) -> windows_core::HRESULT,
    CurrentControlType: usize,
    pub CurrentLocalizedControlType: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CurrentName: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CurrentAcceleratorKey: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CurrentAccessKey: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CurrentHasKeyboardFocus: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub CurrentIsKeyboardFocusable: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub CurrentIsEnabled: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub CurrentAutomationId: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CurrentClassName: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CurrentHelpText: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CurrentCulture:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut i32) -> windows_core::HRESULT,
    pub CurrentIsControlElement: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub CurrentIsContentElement: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub CurrentIsPassword: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub CurrentNativeWindowHandle:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut UIA_HWND) -> windows_core::HRESULT,
    pub CurrentItemType: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CurrentIsOffscreen: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    CurrentOrientation: usize,
    pub CurrentFrameworkId: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CurrentIsRequiredForForm: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub CurrentItemStatus: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    CurrentBoundingRectangle: usize,
    pub CurrentLabeledBy: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CurrentAriaRole: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CurrentAriaProperties: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CurrentIsDataValidForForm: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    CurrentControllerFor: usize,
    CurrentDescribedBy: usize,
    CurrentFlowsTo: usize,
    pub CurrentProviderDescription: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CachedProcessId:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut i32) -> windows_core::HRESULT,
    CachedControlType: usize,
    pub CachedLocalizedControlType: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CachedName: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CachedAcceleratorKey: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CachedAccessKey: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CachedHasKeyboardFocus: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub CachedIsKeyboardFocusable: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub CachedIsEnabled: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub CachedAutomationId: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CachedClassName: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CachedHelpText: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CachedCulture:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut i32) -> windows_core::HRESULT,
    pub CachedIsControlElement: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub CachedIsContentElement: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub CachedIsPassword: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub CachedNativeWindowHandle:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut UIA_HWND) -> windows_core::HRESULT,
    pub CachedItemType: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CachedIsOffscreen: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    CachedOrientation: usize,
    pub CachedFrameworkId: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CachedIsRequiredForForm: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub CachedItemStatus: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    CachedBoundingRectangle: usize,
    pub CachedLabeledBy: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CachedAriaRole: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CachedAriaProperties: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CachedIsDataValidForForm: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    CachedControllerFor: usize,
    CachedDescribedBy: usize,
    CachedFlowsTo: usize,
    pub CachedProviderDescription: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    GetClickablePoint: usize,
}
impl windows_core::RuntimeName for IUIAutomationElement {}
windows_core::imp::define_interface!(
    IUIAutomationScrollPattern,
    IUIAutomationScrollPattern_Vtbl,
    0x88f4d42a_e881_459d_a77c_73bbbb7e02dc
);
windows_core::imp::interface_hierarchy!(IUIAutomationScrollPattern, windows_core::IUnknown);
impl IUIAutomationScrollPattern {
    pub unsafe fn SetScrollPercent(
        &self,
        horizontalpercent: f64,
        verticalpercent: f64,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetScrollPercent)(
                windows_core::Interface::as_raw(self),
                horizontalpercent,
                verticalpercent,
            )
        }
    }
    pub unsafe fn CurrentHorizontalScrollPercent(&self) -> windows_core::Result<f64> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentHorizontalScrollPercent)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CurrentVerticalScrollPercent(&self) -> windows_core::Result<f64> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentVerticalScrollPercent)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CurrentHorizontalViewSize(&self) -> windows_core::Result<f64> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentHorizontalViewSize)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CurrentVerticalViewSize(&self) -> windows_core::Result<f64> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentVerticalViewSize)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CurrentHorizontallyScrollable(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentHorizontallyScrollable)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CurrentVerticallyScrollable(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CurrentVerticallyScrollable)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CachedHorizontalScrollPercent(&self) -> windows_core::Result<f64> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedHorizontalScrollPercent)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CachedVerticalScrollPercent(&self) -> windows_core::Result<f64> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedVerticalScrollPercent)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CachedHorizontalViewSize(&self) -> windows_core::Result<f64> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedHorizontalViewSize)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CachedVerticalViewSize(&self) -> windows_core::Result<f64> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedVerticalViewSize)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CachedHorizontallyScrollable(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedHorizontallyScrollable)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn CachedVerticallyScrollable(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CachedVerticallyScrollable)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
}
#[repr(C)]
pub struct IUIAutomationScrollPattern_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    Scroll: usize,
    pub SetScrollPercent:
        unsafe extern "system" fn(*mut core::ffi::c_void, f64, f64) -> windows_core::HRESULT,
    pub CurrentHorizontalScrollPercent:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut f64) -> windows_core::HRESULT,
    pub CurrentVerticalScrollPercent:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut f64) -> windows_core::HRESULT,
    pub CurrentHorizontalViewSize:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut f64) -> windows_core::HRESULT,
    pub CurrentVerticalViewSize:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut f64) -> windows_core::HRESULT,
    pub CurrentHorizontallyScrollable: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub CurrentVerticallyScrollable: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub CachedHorizontalScrollPercent:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut f64) -> windows_core::HRESULT,
    pub CachedVerticalScrollPercent:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut f64) -> windows_core::HRESULT,
    pub CachedHorizontalViewSize:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut f64) -> windows_core::HRESULT,
    pub CachedVerticalViewSize:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut f64) -> windows_core::HRESULT,
    pub CachedHorizontallyScrollable: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub CachedVerticallyScrollable: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
}
impl windows_core::RuntimeName for IUIAutomationScrollPattern {}
windows_core::imp::define_interface!(
    IXmlDocument,
    IXmlDocument_Vtbl,
    0xf7f3a506_1e87_42d6_bcfb_b8c809fa5494
);
impl windows_core::RuntimeType for IXmlDocument {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"Windows.Data.Xml.Dom.IXmlDocument");
}
#[repr(C)]
pub struct IXmlDocument_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    Doctype: usize,
    Implementation: usize,
    DocumentElement: usize,
    CreateElement: usize,
    CreateDocumentFragment: usize,
    CreateTextNode: usize,
    CreateComment: usize,
    CreateProcessingInstruction: usize,
    CreateAttribute: usize,
    CreateEntityReference: usize,
    GetElementsByTagName: usize,
    CreateCDataSection: usize,
    pub DocumentUri: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    CreateAttributeNS: usize,
    CreateElementNS: usize,
    GetElementById: usize,
    pub ImportNode: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        bool,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IXmlDocumentIO,
    IXmlDocumentIO_Vtbl,
    0x6cd0e74e_ee65_4489_9ebf_ca43e87ba637
);
impl windows_core::RuntimeType for IXmlDocumentIO {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"Windows.Data.Xml.Dom.IXmlDocumentIO");
}
#[repr(C)]
pub struct IXmlDocumentIO_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub LoadXml: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IXmlDocumentIO2,
    IXmlDocumentIO2_Vtbl,
    0x5d034661_7bd8_4ad5_9ebf_81e6347263b1
);
impl windows_core::RuntimeType for IXmlDocumentIO2 {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"Windows.Data.Xml.Dom.IXmlDocumentIO2");
}
#[repr(C)]
pub struct IXmlDocumentIO2_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
}
windows_core::imp::define_interface!(
    IXmlDocumentStatics,
    IXmlDocumentStatics_Vtbl,
    0x5543d254_d757_4b79_9539_232b18f50bf1
);
impl windows_core::RuntimeType for IXmlDocumentStatics {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"Windows.Data.Xml.Dom.IXmlDocumentStatics");
}
#[repr(C)]
pub struct IXmlDocumentStatics_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
}
windows_core::imp::define_interface!(
    IXmlNode,
    IXmlNode_Vtbl,
    0x1c741d59_2122_47d5_a856_83f3d4214875
);
impl windows_core::RuntimeType for IXmlNode {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"Windows.Data.Xml.Dom.IXmlNode");
}
windows_core::imp::interface_hierarchy!(
    IXmlNode,
    windows_core::IUnknown,
    windows_core::IInspectable
);
windows_core::imp::required_hierarchy!(IXmlNode, IXmlNodeSelector, IXmlNodeSerializer);
impl IXmlNode {
    pub fn NodeValue(&self) -> windows_core::Result<windows_core::IInspectable> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).NodeValue)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn SetNodeValue<P0>(&self, value: P0) -> windows_core::Result<()>
    where
        P0: windows_core::Param<windows_core::IInspectable>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).SetNodeValue)(
                windows_core::Interface::as_raw(self),
                value.param().abi(),
            )
            .ok()
        }
    }
    pub fn NodeName(&self) -> windows_core::Result<windows_core::HSTRING> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).NodeName)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn ParentNode(&self) -> windows_core::Result<Self> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).ParentNode)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn FirstChild(&self) -> windows_core::Result<Self> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).FirstChild)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn LastChild(&self) -> windows_core::Result<Self> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).LastChild)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn PreviousSibling(&self) -> windows_core::Result<Self> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).PreviousSibling)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn NextSibling(&self) -> windows_core::Result<Self> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).NextSibling)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn HasChildNodes(&self) -> windows_core::Result<bool> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).HasChildNodes)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn OwnerDocument(&self) -> windows_core::Result<XmlDocument> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).OwnerDocument)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn InsertBefore<P0, P1>(
        &self,
        newchild: P0,
        referencechild: P1,
    ) -> windows_core::Result<Self>
    where
        P0: windows_core::Param<Self>,
        P1: windows_core::Param<Self>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).InsertBefore)(
                windows_core::Interface::as_raw(self),
                newchild.param().abi(),
                referencechild.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn ReplaceChild<P0, P1>(
        &self,
        newchild: P0,
        referencechild: P1,
    ) -> windows_core::Result<Self>
    where
        P0: windows_core::Param<Self>,
        P1: windows_core::Param<Self>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).ReplaceChild)(
                windows_core::Interface::as_raw(self),
                newchild.param().abi(),
                referencechild.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn RemoveChild<P0>(&self, childnode: P0) -> windows_core::Result<Self>
    where
        P0: windows_core::Param<Self>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).RemoveChild)(
                windows_core::Interface::as_raw(self),
                childnode.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn AppendChild<P0>(&self, newchild: P0) -> windows_core::Result<Self>
    where
        P0: windows_core::Param<Self>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).AppendChild)(
                windows_core::Interface::as_raw(self),
                newchild.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn CloneNode(&self, deep: bool) -> windows_core::Result<Self> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CloneNode)(
                windows_core::Interface::as_raw(self),
                deep,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn NamespaceUri(&self) -> windows_core::Result<windows_core::IInspectable> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).NamespaceUri)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn LocalName(&self) -> windows_core::Result<windows_core::IInspectable> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).LocalName)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn Prefix(&self) -> windows_core::Result<windows_core::IInspectable> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).Prefix)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn Normalize(&self) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).Normalize)(windows_core::Interface::as_raw(self))
                .ok()
        }
    }
    pub fn SetPrefix<P0>(&self, value: P0) -> windows_core::Result<()>
    where
        P0: windows_core::Param<windows_core::IInspectable>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).SetPrefix)(
                windows_core::Interface::as_raw(self),
                value.param().abi(),
            )
            .ok()
        }
    }
    pub fn SelectSingleNode(&self, xpath: &windows_core::HSTRING) -> windows_core::Result<Self> {
        let this = &windows_core::Interface::cast::<IXmlNodeSelector>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).SelectSingleNode)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(xpath),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn SelectSingleNodeNS<P1>(
        &self,
        xpath: &windows_core::HSTRING,
        namespaces: P1,
    ) -> windows_core::Result<Self>
    where
        P1: windows_core::Param<windows_core::IInspectable>,
    {
        let this = &windows_core::Interface::cast::<IXmlNodeSelector>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).SelectSingleNodeNS)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(xpath),
                namespaces.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn GetXml(&self) -> windows_core::Result<windows_core::HSTRING> {
        let this = &windows_core::Interface::cast::<IXmlNodeSerializer>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).GetXml)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn InnerText(&self) -> windows_core::Result<windows_core::HSTRING> {
        let this = &windows_core::Interface::cast::<IXmlNodeSerializer>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).InnerText)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn SetInnerText(&self, value: &windows_core::HSTRING) -> windows_core::Result<()> {
        let this = &windows_core::Interface::cast::<IXmlNodeSerializer>(self)?;
        unsafe {
            (windows_core::Interface::vtable(this).SetInnerText)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(value),
            )
            .ok()
        }
    }
}
impl windows_core::RuntimeName for IXmlNode {
    const NAME: &'static str = "Windows.Data.Xml.Dom.IXmlNode";
}
#[repr(C)]
pub struct IXmlNode_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub NodeValue: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SetNodeValue: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    NodeType: usize,
    pub NodeName: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ParentNode: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    ChildNodes: usize,
    pub FirstChild: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub LastChild: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub PreviousSibling: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub NextSibling: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    Attributes: usize,
    pub HasChildNodes:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
    pub OwnerDocument: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub InsertBefore: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ReplaceChild: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub RemoveChild: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub AppendChild: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CloneNode: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        bool,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub NamespaceUri: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub LocalName: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub Prefix: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub Normalize: unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
    pub SetPrefix: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IXmlNodeSelector,
    IXmlNodeSelector_Vtbl,
    0x63dbba8b_d0db_4fe1_b745_f9433afdc25b
);
impl windows_core::RuntimeType for IXmlNodeSelector {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"Windows.Data.Xml.Dom.IXmlNodeSelector");
}
windows_core::imp::interface_hierarchy!(
    IXmlNodeSelector,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl IXmlNodeSelector {
    pub fn SelectSingleNode(
        &self,
        xpath: &windows_core::HSTRING,
    ) -> windows_core::Result<IXmlNode> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).SelectSingleNode)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(xpath),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn SelectSingleNodeNS<P1>(
        &self,
        xpath: &windows_core::HSTRING,
        namespaces: P1,
    ) -> windows_core::Result<IXmlNode>
    where
        P1: windows_core::Param<windows_core::IInspectable>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).SelectSingleNodeNS)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(xpath),
                namespaces.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
}
impl windows_core::RuntimeName for IXmlNodeSelector {
    const NAME: &'static str = "Windows.Data.Xml.Dom.IXmlNodeSelector";
}
#[repr(C)]
pub struct IXmlNodeSelector_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub SelectSingleNode: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    SelectNodes: usize,
    pub SelectSingleNodeNS: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IXmlNodeSerializer,
    IXmlNodeSerializer_Vtbl,
    0x5cc5b382_e6dd_4991_abef_06d8d2e7bd0c
);
impl windows_core::RuntimeType for IXmlNodeSerializer {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
    const NAME: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"Windows.Data.Xml.Dom.IXmlNodeSerializer");
}
windows_core::imp::interface_hierarchy!(
    IXmlNodeSerializer,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl IXmlNodeSerializer {
    pub fn GetXml(&self) -> windows_core::Result<windows_core::HSTRING> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetXml)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn InnerText(&self) -> windows_core::Result<windows_core::HSTRING> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).InnerText)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn SetInnerText(&self, value: &windows_core::HSTRING) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).SetInnerText)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(value),
            )
            .ok()
        }
    }
}
impl windows_core::RuntimeName for IXmlNodeSerializer {
    const NAME: &'static str = "Windows.Data.Xml.Dom.IXmlNodeSerializer";
}
pub trait IXmlNodeSerializer_Impl: windows_core::IUnknownImpl {
    fn GetXml(&self) -> windows_core::Result<windows_core::HSTRING>;
    fn InnerText(&self) -> windows_core::Result<windows_core::HSTRING>;
    fn SetInnerText(&self, value: &windows_core::HSTRING) -> windows_core::Result<()>;
}
impl IXmlNodeSerializer_Vtbl {
    pub const fn new<Identity: IXmlNodeSerializer_Impl, const OFFSET: isize>() -> Self {
        unsafe extern "system" fn GetXml<Identity: IXmlNodeSerializer_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            result__: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IXmlNodeSerializer_Impl::GetXml(this) {
                    Ok(ok__) => {
                        result__.write(core::mem::transmute_copy(&ok__));
                        core::mem::forget(ok__);
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn InnerText<
            Identity: IXmlNodeSerializer_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            result__: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IXmlNodeSerializer_Impl::InnerText(this) {
                    Ok(ok__) => {
                        result__.write(core::mem::transmute_copy(&ok__));
                        core::mem::forget(ok__);
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn SetInnerText<
            Identity: IXmlNodeSerializer_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            value: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IXmlNodeSerializer_Impl::SetInnerText(this, core::mem::transmute(&value)).into()
            }
        }
        Self {
            base__: windows_core::IInspectable_Vtbl::new::<Identity, IXmlNodeSerializer, OFFSET>(),
            GetXml: GetXml::<Identity, OFFSET>,
            InnerText: InnerText::<Identity, OFFSET>,
            SetInnerText: SetInnerText::<Identity, OFFSET>,
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<IXmlNodeSerializer as windows_core::Interface>::IID
    }
}
#[repr(C)]
pub struct IXmlNodeSerializer_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub GetXml: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub InnerText: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SetInnerText: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
pub type LPSAFEARRAY = *mut SAFEARRAY;
pub type LPVERSIONEDSTREAM = *mut VERSIONEDSTREAM;
pub const MMDeviceEnumerator: windows_core::GUID =
    windows_core::GUID::from_u128(0xbcde0395_e52f_467c_8e3d_c4579291692e);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MediaPlaybackAutoRepeatMode(pub i32);
impl MediaPlaybackAutoRepeatMode {
    pub const None: Self = Self(0);
    pub const Track: Self = Self(1);
    pub const List: Self = Self(2);
}
impl windows_core::imp::TypeKind for MediaPlaybackAutoRepeatMode {
    type TypeKind = windows_core::imp::CopyType;
}
impl windows_core::RuntimeType for MediaPlaybackAutoRepeatMode {
    const SIGNATURE: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::from_slice(
        b"enum(Windows.Media.MediaPlaybackAutoRepeatMode;i4)",
    );
    const NAME: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"Windows.Media.MediaPlaybackAutoRepeatMode");
}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MediaPlaybackType(pub i32);
impl MediaPlaybackType {
    pub const Unknown: Self = Self(0);
    pub const Music: Self = Self(1);
    pub const Video: Self = Self(2);
    pub const Image: Self = Self(3);
}
impl windows_core::imp::TypeKind for MediaPlaybackType {
    type TypeKind = windows_core::imp::CopyType;
}
impl windows_core::RuntimeType for MediaPlaybackType {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"enum(Windows.Media.MediaPlaybackType;i4)");
    const NAME: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::from_slice(b"Windows.Media.MediaPlaybackType");
}
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaPropertiesChangedEventArgs(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    MediaPropertiesChangedEventArgs,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl windows_core::RuntimeType for MediaPropertiesChangedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_class::<Self, IMediaPropertiesChangedEventArgs>();
}
unsafe impl windows_core::Interface for MediaPropertiesChangedEventArgs {
    type Vtable = <IMediaPropertiesChangedEventArgs as windows_core::Interface>::Vtable;
    const IID: windows_core::GUID =
        <IMediaPropertiesChangedEventArgs as windows_core::Interface>::IID;
}
impl windows_core::RuntimeName for MediaPropertiesChangedEventArgs {
    const NAME: &'static str = "Windows.Media.Control.MediaPropertiesChangedEventArgs";
}
unsafe impl Send for MediaPropertiesChangedEventArgs {}
unsafe impl Sync for MediaPropertiesChangedEventArgs {}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PATTERNID(pub i32);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PROPERTYID(pub i32);
#[repr(C)]
pub struct PROPVARIANT {
    pub Anonymous: PROPVARIANT_0,
}
impl Clone for PROPVARIANT {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for PROPVARIANT {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
pub union PROPVARIANT_0 {
    pub Anonymous: core::mem::ManuallyDrop<PROPVARIANT_0_0>,
    pub decVal: DECIMAL,
}
impl Clone for PROPVARIANT_0 {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for PROPVARIANT_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
pub struct PROPVARIANT_0_0 {
    pub vt: VARTYPE,
    pub wReserved1: PROPVAR_PAD1,
    pub wReserved2: PROPVAR_PAD2,
    pub wReserved3: PROPVAR_PAD3,
    pub Anonymous: PROPVARIANT_0_0_0,
}
impl Clone for PROPVARIANT_0_0 {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for PROPVARIANT_0_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
pub union PROPVARIANT_0_0_0 {
    pub cVal: i8,
    pub bVal: u8,
    pub iVal: i16,
    pub uiVal: u16,
    pub lVal: i32,
    pub ulVal: u32,
    pub intVal: i32,
    pub uintVal: u32,
    pub hVal: i64,
    pub uhVal: u64,
    pub fltVal: f32,
    pub dblVal: f64,
    pub boolVal: VARIANT_BOOL,
    pub __OBSOLETE__VARIANT_BOOL: VARIANT_BOOL,
    pub scode: SCODE,
    pub cyVal: CY,
    pub date: f64,
    pub filetime: FILETIME,
    pub puuid: *mut windows_core::GUID,
    pub pclipdata: *mut CLIPDATA,
    pub bstrVal: core::mem::ManuallyDrop<windows_core::BSTR>,
    pub bstrblobVal: BSTRBLOB,
    pub blob: BLOB,
    pub pszVal: windows_core::PSTR,
    pub pwszVal: windows_core::PWSTR,
    pub punkVal: core::mem::ManuallyDrop<Option<windows_core::IUnknown>>,
    pub pdispVal: core::mem::ManuallyDrop<Option<IDispatch>>,
    pub pStream: core::mem::ManuallyDrop<Option<IStream>>,
    pub pStorage: core::mem::ManuallyDrop<Option<IStorage>>,
    pub pVersionedStream: LPVERSIONEDSTREAM,
    pub parray: LPSAFEARRAY,
    pub cac: CAC,
    pub caub: CAUB,
    pub cai: CAI,
    pub caui: CAUI,
    pub cal: CAL,
    pub caul: CAUL,
    pub cah: CAH,
    pub cauh: CAUH,
    pub caflt: CAFLT,
    pub cadbl: CADBL,
    pub cabool: CABOOL,
    pub cascode: CASCODE,
    pub cacy: CACY,
    pub cadate: CADATE,
    pub cafiletime: CAFILETIME,
    pub cauuid: CACLSID,
    pub caclipdata: CACLIPDATA,
    pub cabstr: CABSTR,
    pub cabstrblob: CABSTRBLOB,
    pub calpstr: CALPSTR,
    pub calpwstr: CALPWSTR,
    pub capropvar: CAPROPVARIANT,
    pub pcVal: *mut i8,
    pub pbVal: *mut u8,
    pub piVal: *mut i16,
    pub puiVal: *mut u16,
    pub plVal: *mut i32,
    pub pulVal: *mut u32,
    pub pintVal: *mut i32,
    pub puintVal: *mut u32,
    pub pfltVal: *mut f32,
    pub pdblVal: *mut f64,
    pub pboolVal: *mut VARIANT_BOOL,
    pub pdecVal: *mut DECIMAL,
    pub pscode: *mut SCODE,
    pub pcyVal: *mut CY,
    pub pdate: *mut f64,
    pub pbstrVal: *mut windows_core::BSTR,
    pub ppunkVal: *mut Option<windows_core::IUnknown>,
    pub ppdispVal: *mut Option<IDispatch>,
    pub pparray: *mut LPSAFEARRAY,
    pub pvarVal: *mut PROPVARIANT,
}
impl Clone for PROPVARIANT_0_0_0 {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for PROPVARIANT_0_0_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PROPVAR_PAD1(pub u16);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PROPVAR_PAD2(pub u16);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PROPVAR_PAD3(pub u16);
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackInfoChangedEventArgs(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    PlaybackInfoChangedEventArgs,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl windows_core::RuntimeType for PlaybackInfoChangedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_class::<Self, IPlaybackInfoChangedEventArgs>();
}
unsafe impl windows_core::Interface for PlaybackInfoChangedEventArgs {
    type Vtable = <IPlaybackInfoChangedEventArgs as windows_core::Interface>::Vtable;
    const IID: windows_core::GUID = <IPlaybackInfoChangedEventArgs as windows_core::Interface>::IID;
}
impl windows_core::RuntimeName for PlaybackInfoChangedEventArgs {
    const NAME: &'static str = "Windows.Media.Control.PlaybackInfoChangedEventArgs";
}
unsafe impl Send for PlaybackInfoChangedEventArgs {}
unsafe impl Sync for PlaybackInfoChangedEventArgs {}
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SAFEARRAY {
    pub cDims: u16,
    pub fFeatures: u16,
    pub cbElements: u32,
    pub cLocks: u32,
    pub pvData: *mut core::ffi::c_void,
    pub rgsabound: [SAFEARRAYBOUND; 1],
}
impl Default for SAFEARRAY {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SAFEARRAYBOUND {
    pub cElements: u32,
    pub lLbound: i32,
}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SCODE(pub i32);
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionsChangedEventArgs(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    SessionsChangedEventArgs,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl windows_core::RuntimeType for SessionsChangedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_class::<Self, ISessionsChangedEventArgs>();
}
unsafe impl windows_core::Interface for SessionsChangedEventArgs {
    type Vtable = <ISessionsChangedEventArgs as windows_core::Interface>::Vtable;
    const IID: windows_core::GUID = <ISessionsChangedEventArgs as windows_core::Interface>::IID;
}
impl windows_core::RuntimeName for SessionsChangedEventArgs {
    const NAME: &'static str = "Windows.Media.Control.SessionsChangedEventArgs";
}
unsafe impl Send for SessionsChangedEventArgs {}
unsafe impl Sync for SessionsChangedEventArgs {}
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimelinePropertiesChangedEventArgs(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    TimelinePropertiesChangedEventArgs,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl windows_core::RuntimeType for TimelinePropertiesChangedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_class::<Self, ITimelinePropertiesChangedEventArgs>();
}
unsafe impl windows_core::Interface for TimelinePropertiesChangedEventArgs {
    type Vtable = <ITimelinePropertiesChangedEventArgs as windows_core::Interface>::Vtable;
    const IID: windows_core::GUID =
        <ITimelinePropertiesChangedEventArgs as windows_core::Interface>::IID;
}
impl windows_core::RuntimeName for TimelinePropertiesChangedEventArgs {
    const NAME: &'static str = "Windows.Media.Control.TimelinePropertiesChangedEventArgs";
}
unsafe impl Send for TimelinePropertiesChangedEventArgs {}
unsafe impl Sync for TimelinePropertiesChangedEventArgs {}
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToastActivatedEventArgs(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    ToastActivatedEventArgs,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl ToastActivatedEventArgs {
    pub fn Arguments(&self) -> windows_core::Result<windows_core::HSTRING> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).Arguments)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn UserInput(&self) -> windows_core::Result<ValueSet> {
        let this = &windows_core::Interface::cast::<IToastActivatedEventArgs2>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).UserInput)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
}
impl windows_core::RuntimeType for ToastActivatedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_class::<Self, IToastActivatedEventArgs>();
}
unsafe impl windows_core::Interface for ToastActivatedEventArgs {
    type Vtable = <IToastActivatedEventArgs as windows_core::Interface>::Vtable;
    const IID: windows_core::GUID = <IToastActivatedEventArgs as windows_core::Interface>::IID;
}
impl windows_core::RuntimeName for ToastActivatedEventArgs {
    const NAME: &'static str = "Windows.UI.Notifications.ToastActivatedEventArgs";
}
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToastDismissedEventArgs(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    ToastDismissedEventArgs,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl windows_core::RuntimeType for ToastDismissedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_class::<Self, IToastDismissedEventArgs>();
}
unsafe impl windows_core::Interface for ToastDismissedEventArgs {
    type Vtable = <IToastDismissedEventArgs as windows_core::Interface>::Vtable;
    const IID: windows_core::GUID = <IToastDismissedEventArgs as windows_core::Interface>::IID;
}
impl windows_core::RuntimeName for ToastDismissedEventArgs {
    const NAME: &'static str = "Windows.UI.Notifications.ToastDismissedEventArgs";
}
unsafe impl Send for ToastDismissedEventArgs {}
unsafe impl Sync for ToastDismissedEventArgs {}
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToastFailedEventArgs(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    ToastFailedEventArgs,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl ToastFailedEventArgs {
    pub fn ErrorCode(&self) -> windows_core::Result<windows_core::HRESULT> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).ErrorCode)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
}
impl windows_core::RuntimeType for ToastFailedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_class::<Self, IToastFailedEventArgs>();
}
unsafe impl windows_core::Interface for ToastFailedEventArgs {
    type Vtable = <IToastFailedEventArgs as windows_core::Interface>::Vtable;
    const IID: windows_core::GUID = <IToastFailedEventArgs as windows_core::Interface>::IID;
}
impl windows_core::RuntimeName for ToastFailedEventArgs {
    const NAME: &'static str = "Windows.UI.Notifications.ToastFailedEventArgs";
}
unsafe impl Send for ToastFailedEventArgs {}
unsafe impl Sync for ToastFailedEventArgs {}
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToastNotification(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    ToastNotification,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl ToastNotification {
    pub fn Content(&self) -> windows_core::Result<XmlDocument> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).Content)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn SetExpirationTime(
        &self,
        value: Option<windows_time::DateTime>,
    ) -> windows_core::Result<()> {
        let value__ =
            value.map(<windows_reference::IReference<windows_time::DateTime> as From<_>>::from);
        unsafe {
            (windows_core::Interface::vtable(self).SetExpirationTime)(
                windows_core::Interface::as_raw(self),
                windows_core::Param::param(value__.as_ref()).abi(),
            )
            .ok()
        }
    }
    pub fn ExpirationTime(&self) -> windows_core::Result<windows_time::DateTime> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).ExpirationTime)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
            .and_then(|r__: windows_reference::IReference<windows_time::DateTime>| r__.Value())
        }
    }
    pub fn Dismissed<F>(&self, handler: F) -> windows_core::Result<windows_core::EventRevoker>
    where
        F: Fn(windows_core::Ref<Self>, windows_core::Ref<ToastDismissedEventArgs>) + Send + 'static,
    {
        let handler = <TypedEventHandler<Self, ToastDismissedEventArgs>>::new(move |a0, a1| {
            handler(a0, a1);
            Ok(())
        });
        unsafe {
            let mut result__ = core::mem::zeroed();
            let token__ = (windows_core::Interface::vtable(self).Dismissed)(
                windows_core::Interface::as_raw(self),
                windows_core::Interface::as_raw(&handler),
                &mut result__,
            )
            .map(|| result__)?;
            Ok(windows_core::EventRevoker::new(
                self.clone(),
                token__,
                windows_core::Interface::vtable(self).RemoveDismissed,
            ))
        }
    }
    pub fn Activated<F>(&self, handler: F) -> windows_core::Result<windows_core::EventRevoker>
    where
        F: Fn(windows_core::Ref<Self>, windows_core::Ref<windows_core::IInspectable>)
            + Send
            + 'static,
    {
        let handler = <TypedEventHandler<Self, windows_core::IInspectable>>::new(move |a0, a1| {
            handler(a0, a1);
            Ok(())
        });
        unsafe {
            let mut result__ = core::mem::zeroed();
            let token__ = (windows_core::Interface::vtable(self).Activated)(
                windows_core::Interface::as_raw(self),
                windows_core::Interface::as_raw(&handler),
                &mut result__,
            )
            .map(|| result__)?;
            Ok(windows_core::EventRevoker::new(
                self.clone(),
                token__,
                windows_core::Interface::vtable(self).RemoveActivated,
            ))
        }
    }
    pub fn Failed<F>(&self, handler: F) -> windows_core::Result<windows_core::EventRevoker>
    where
        F: Fn(windows_core::Ref<Self>, windows_core::Ref<ToastFailedEventArgs>) + Send + 'static,
    {
        let handler = <TypedEventHandler<Self, ToastFailedEventArgs>>::new(move |a0, a1| {
            handler(a0, a1);
            Ok(())
        });
        unsafe {
            let mut result__ = core::mem::zeroed();
            let token__ = (windows_core::Interface::vtable(self).Failed)(
                windows_core::Interface::as_raw(self),
                windows_core::Interface::as_raw(&handler),
                &mut result__,
            )
            .map(|| result__)?;
            Ok(windows_core::EventRevoker::new(
                self.clone(),
                token__,
                windows_core::Interface::vtable(self).RemoveFailed,
            ))
        }
    }
    pub fn SetTag(&self, value: &windows_core::HSTRING) -> windows_core::Result<()> {
        let this = &windows_core::Interface::cast::<IToastNotification2>(self)?;
        unsafe {
            (windows_core::Interface::vtable(this).SetTag)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(value),
            )
            .ok()
        }
    }
    pub fn Tag(&self) -> windows_core::Result<windows_core::HSTRING> {
        let this = &windows_core::Interface::cast::<IToastNotification2>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).Tag)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn SetGroup(&self, value: &windows_core::HSTRING) -> windows_core::Result<()> {
        let this = &windows_core::Interface::cast::<IToastNotification2>(self)?;
        unsafe {
            (windows_core::Interface::vtable(this).SetGroup)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(value),
            )
            .ok()
        }
    }
    pub fn Group(&self) -> windows_core::Result<windows_core::HSTRING> {
        let this = &windows_core::Interface::cast::<IToastNotification2>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).Group)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn SetSuppressPopup(&self, value: bool) -> windows_core::Result<()> {
        let this = &windows_core::Interface::cast::<IToastNotification2>(self)?;
        unsafe {
            (windows_core::Interface::vtable(this).SetSuppressPopup)(
                windows_core::Interface::as_raw(this),
                value,
            )
            .ok()
        }
    }
    pub fn SuppressPopup(&self) -> windows_core::Result<bool> {
        let this = &windows_core::Interface::cast::<IToastNotification2>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).SuppressPopup)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn RemoteId(&self) -> windows_core::Result<windows_core::HSTRING> {
        let this = &windows_core::Interface::cast::<IToastNotification3>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).RemoteId)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn SetRemoteId(&self, value: &windows_core::HSTRING) -> windows_core::Result<()> {
        let this = &windows_core::Interface::cast::<IToastNotification3>(self)?;
        unsafe {
            (windows_core::Interface::vtable(this).SetRemoteId)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(value),
            )
            .ok()
        }
    }
    pub fn ExpiresOnReboot(&self) -> windows_core::Result<bool> {
        let this = &windows_core::Interface::cast::<IToastNotification6>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).ExpiresOnReboot)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn SetExpiresOnReboot(&self, value: bool) -> windows_core::Result<()> {
        let this = &windows_core::Interface::cast::<IToastNotification6>(self)?;
        unsafe {
            (windows_core::Interface::vtable(this).SetExpiresOnReboot)(
                windows_core::Interface::as_raw(this),
                value,
            )
            .ok()
        }
    }
    pub fn CreateToastNotification<P0>(content: P0) -> windows_core::Result<Self>
    where
        P0: windows_core::Param<XmlDocument>,
    {
        Self::IToastNotificationFactory(|this| unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).CreateToastNotification)(
                windows_core::Interface::as_raw(this),
                content.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        })
    }
    fn IToastNotificationFactory<
        R,
        F: FnOnce(&IToastNotificationFactory) -> windows_core::Result<R>,
    >(
        callback: F,
    ) -> windows_core::Result<R> {
        static SHARED: windows_core::imp::FactoryCache<
            ToastNotification,
            IToastNotificationFactory,
        > = windows_core::imp::FactoryCache::new();
        SHARED.call(callback)
    }
}
impl windows_core::RuntimeType for ToastNotification {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_class::<Self, IToastNotification>();
}
unsafe impl windows_core::Interface for ToastNotification {
    type Vtable = <IToastNotification as windows_core::Interface>::Vtable;
    const IID: windows_core::GUID = <IToastNotification as windows_core::Interface>::IID;
}
impl windows_core::RuntimeName for ToastNotification {
    const NAME: &'static str = "Windows.UI.Notifications.ToastNotification";
}
unsafe impl Send for ToastNotification {}
unsafe impl Sync for ToastNotification {}
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToastNotificationHistory(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    ToastNotificationHistory,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl ToastNotificationHistory {
    pub fn RemoveGroup(&self, group: &windows_core::HSTRING) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).RemoveGroup)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(group),
            )
            .ok()
        }
    }
    pub fn RemoveGroupWithId(
        &self,
        group: &windows_core::HSTRING,
        applicationid: &windows_core::HSTRING,
    ) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).RemoveGroupWithId)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(group),
                core::mem::transmute_copy(applicationid),
            )
            .ok()
        }
    }
    pub fn RemoveGroupedTagWithId(
        &self,
        tag: &windows_core::HSTRING,
        group: &windows_core::HSTRING,
        applicationid: &windows_core::HSTRING,
    ) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).RemoveGroupedTagWithId)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(tag),
                core::mem::transmute_copy(group),
                core::mem::transmute_copy(applicationid),
            )
            .ok()
        }
    }
    pub fn RemoveGroupedTag(
        &self,
        tag: &windows_core::HSTRING,
        group: &windows_core::HSTRING,
    ) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).RemoveGroupedTag)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(tag),
                core::mem::transmute_copy(group),
            )
            .ok()
        }
    }
    pub fn Remove(&self, tag: &windows_core::HSTRING) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).Remove)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(tag),
            )
            .ok()
        }
    }
    pub fn Clear(&self) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).Clear)(windows_core::Interface::as_raw(self))
                .ok()
        }
    }
    pub fn ClearWithId(&self, applicationid: &windows_core::HSTRING) -> windows_core::Result<()> {
        unsafe {
            (windows_core::Interface::vtable(self).ClearWithId)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(applicationid),
            )
            .ok()
        }
    }
    pub fn GetHistory(
        &self,
    ) -> windows_core::Result<windows_collections::IVectorView<ToastNotification>> {
        let this = &windows_core::Interface::cast::<IToastNotificationHistory2>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).GetHistory)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn GetHistoryWithId(
        &self,
        applicationid: &windows_core::HSTRING,
    ) -> windows_core::Result<windows_collections::IVectorView<ToastNotification>> {
        let this = &windows_core::Interface::cast::<IToastNotificationHistory2>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).GetHistoryWithId)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(applicationid),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
}
impl windows_core::RuntimeType for ToastNotificationHistory {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_class::<Self, IToastNotificationHistory>();
}
unsafe impl windows_core::Interface for ToastNotificationHistory {
    type Vtable = <IToastNotificationHistory as windows_core::Interface>::Vtable;
    const IID: windows_core::GUID = <IToastNotificationHistory as windows_core::Interface>::IID;
}
impl windows_core::RuntimeName for ToastNotificationHistory {
    const NAME: &'static str = "Windows.UI.Notifications.ToastNotificationHistory";
}
pub struct ToastNotificationManager;
impl ToastNotificationManager {
    pub fn CreateToastNotifier() -> windows_core::Result<ToastNotifier> {
        Self::IToastNotificationManagerStatics(|this| unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).CreateToastNotifier)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        })
    }
    pub fn CreateToastNotifierWithId(
        applicationid: &windows_core::HSTRING,
    ) -> windows_core::Result<ToastNotifier> {
        Self::IToastNotificationManagerStatics(|this| unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).CreateToastNotifierWithId)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(applicationid),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        })
    }
    pub fn History() -> windows_core::Result<ToastNotificationHistory> {
        Self::IToastNotificationManagerStatics2(|this| unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).History)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        })
    }
    fn IToastNotificationManagerStatics<
        R,
        F: FnOnce(&IToastNotificationManagerStatics) -> windows_core::Result<R>,
    >(
        callback: F,
    ) -> windows_core::Result<R> {
        static SHARED: windows_core::imp::FactoryCache<
            ToastNotificationManager,
            IToastNotificationManagerStatics,
        > = windows_core::imp::FactoryCache::new();
        SHARED.call(callback)
    }
    fn IToastNotificationManagerStatics2<
        R,
        F: FnOnce(&IToastNotificationManagerStatics2) -> windows_core::Result<R>,
    >(
        callback: F,
    ) -> windows_core::Result<R> {
        static SHARED: windows_core::imp::FactoryCache<
            ToastNotificationManager,
            IToastNotificationManagerStatics2,
        > = windows_core::imp::FactoryCache::new();
        SHARED.call(callback)
    }
    fn IToastNotificationManagerStatics4<
        R,
        F: FnOnce(&IToastNotificationManagerStatics4) -> windows_core::Result<R>,
    >(
        callback: F,
    ) -> windows_core::Result<R> {
        static SHARED: windows_core::imp::FactoryCache<
            ToastNotificationManager,
            IToastNotificationManagerStatics4,
        > = windows_core::imp::FactoryCache::new();
        SHARED.call(callback)
    }
    fn IToastNotificationManagerStatics5<
        R,
        F: FnOnce(&IToastNotificationManagerStatics5) -> windows_core::Result<R>,
    >(
        callback: F,
    ) -> windows_core::Result<R> {
        static SHARED: windows_core::imp::FactoryCache<
            ToastNotificationManager,
            IToastNotificationManagerStatics5,
        > = windows_core::imp::FactoryCache::new();
        SHARED.call(callback)
    }
}
impl windows_core::RuntimeName for ToastNotificationManager {
    const NAME: &'static str = "Windows.UI.Notifications.ToastNotificationManager";
}
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToastNotifier(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    ToastNotifier,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl ToastNotifier {
    pub fn Show<P0>(&self, notification: P0) -> windows_core::Result<()>
    where
        P0: windows_core::Param<ToastNotification>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).Show)(
                windows_core::Interface::as_raw(self),
                notification.param().abi(),
            )
            .ok()
        }
    }
    pub fn Hide<P0>(&self, notification: P0) -> windows_core::Result<()>
    where
        P0: windows_core::Param<ToastNotification>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).Hide)(
                windows_core::Interface::as_raw(self),
                notification.param().abi(),
            )
            .ok()
        }
    }
}
impl windows_core::RuntimeType for ToastNotifier {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_class::<Self, IToastNotifier>();
}
unsafe impl windows_core::Interface for ToastNotifier {
    type Vtable = <IToastNotifier as windows_core::Interface>::Vtable;
    const IID: windows_core::GUID = <IToastNotifier as windows_core::Interface>::IID;
}
impl windows_core::RuntimeName for ToastNotifier {
    const NAME: &'static str = "Windows.UI.Notifications.ToastNotifier";
}
unsafe impl Send for ToastNotifier {}
unsafe impl Sync for ToastNotifier {}
pub type TreeScope = i32;
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedEventHandler<TSender, TResult>(
    windows_core::IUnknown,
    core::marker::PhantomData<TSender>,
    core::marker::PhantomData<TResult>,
)
where
    TSender: windows_core::RuntimeType + 'static,
    TResult: windows_core::RuntimeType + 'static;
unsafe impl<
    TSender: windows_core::RuntimeType + 'static,
    TResult: windows_core::RuntimeType + 'static,
> windows_core::Interface for TypedEventHandler<TSender, TResult>
{
    type Vtable = TypedEventHandler_Vtbl<TSender, TResult>;
    const IID: windows_core::GUID =
        windows_core::GUID::from_signature(<Self as windows_core::RuntimeType>::SIGNATURE);
}
impl<TSender: windows_core::RuntimeType + 'static, TResult: windows_core::RuntimeType + 'static>
    windows_core::RuntimeType for TypedEventHandler<TSender, TResult>
{
    const SIGNATURE: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::new()
        .push_slice(b"pinterface({9de1c534-6ae1-11e0-84e1-18a905bcc53f}")
        .push_slice(b";")
        .push_other(TSender::SIGNATURE)
        .push_slice(b";")
        .push_other(TResult::SIGNATURE)
        .push_slice(b")");
}
impl<TSender: windows_core::RuntimeType + 'static, TResult: windows_core::RuntimeType + 'static>
    TypedEventHandler<TSender, TResult>
{
    pub fn new<
        F: Fn(windows_core::Ref<TSender>, windows_core::Ref<TResult>) -> windows_core::Result<()>
            + Send
            + 'static,
    >(
        invoke: F,
    ) -> Self {
        let com = windows_core::imp::DelegateBox::<Self, F>::new(
            &TypedEventHandlerBox::<TSender, TResult, F>::VTABLE,
            invoke,
        );
        unsafe { core::mem::transmute(windows_core::imp::box_new(com)) }
    }
    pub fn Invoke<P0, P1>(&self, sender: P0, args: P1) -> windows_core::Result<()>
    where
        P0: windows_core::Param<TSender>,
        P1: windows_core::Param<TResult>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).Invoke)(
                windows_core::Interface::as_raw(self),
                sender.param().abi(),
                args.param().abi(),
            )
            .ok()
        }
    }
}
#[repr(C)]
pub struct TypedEventHandler_Vtbl<TSender, TResult>
where
    TSender: windows_core::RuntimeType + 'static,
    TResult: windows_core::RuntimeType + 'static,
{
    base__: windows_core::IUnknown_Vtbl,
    Invoke: unsafe extern "system" fn(
        this: *mut core::ffi::c_void,
        sender: windows_core::imp::AbiType<TSender>,
        args: windows_core::imp::AbiType<TResult>,
    ) -> windows_core::HRESULT,
    TSender: core::marker::PhantomData<TSender>,
    TResult: core::marker::PhantomData<TResult>,
}
struct TypedEventHandlerBox<
    TSender,
    TResult,
    F: Fn(windows_core::Ref<TSender>, windows_core::Ref<TResult>) -> windows_core::Result<()>
        + Send
        + 'static,
>(core::marker::PhantomData<(TSender, TResult, fn() -> F)>)
where
    TSender: windows_core::RuntimeType + 'static,
    TResult: windows_core::RuntimeType + 'static;
impl<
    TSender: windows_core::RuntimeType + 'static,
    TResult: windows_core::RuntimeType + 'static,
    F: Fn(windows_core::Ref<TSender>, windows_core::Ref<TResult>) -> windows_core::Result<()>
        + Send
        + 'static,
> TypedEventHandlerBox<TSender, TResult, F>
{
    const VTABLE : TypedEventHandler_Vtbl < TSender , TResult , > = TypedEventHandler_Vtbl::< TSender , TResult , > { base__ : windows_core::IUnknown_Vtbl { QueryInterface : windows_core::imp::DelegateBox::< TypedEventHandler < TSender , TResult > , F >::QueryInterface , AddRef : windows_core::imp::DelegateBox::< TypedEventHandler < TSender , TResult > , F >::AddRef , Release : windows_core::imp::DelegateBox::< TypedEventHandler < TSender , TResult > , F >::Release , } , Invoke : Self::Invoke , TSender : core::marker::PhantomData::< TSender > , TResult : core::marker::PhantomData::< TResult > } ;
    unsafe extern "system" fn Invoke(
        this: *mut core::ffi::c_void,
        sender: windows_core::imp::AbiType<TSender>,
        args: windows_core::imp::AbiType<TResult>,
    ) -> windows_core::HRESULT {
        unsafe {
            let this = &mut *(this as *mut *mut core::ffi::c_void
                as *mut windows_core::imp::DelegateBox<TypedEventHandler<TSender, TResult>, F>);
            (this.invoke)(
                core::mem::transmute_copy(&sender),
                core::mem::transmute_copy(&args),
            )
            .into()
        }
    }
}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct UIA_HWND(pub *mut core::ffi::c_void);
pub type VARENUM = i32;
#[repr(C)]
pub struct VARIANT {
    pub Anonymous: VARIANT_0,
}
impl Clone for VARIANT {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for VARIANT {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
pub union VARIANT_0 {
    pub Anonymous: core::mem::ManuallyDrop<VARIANT_0_0>,
    pub decVal: DECIMAL,
}
impl Clone for VARIANT_0 {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for VARIANT_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
pub struct VARIANT_0_0 {
    pub vt: VARTYPE,
    pub wReserved1: u16,
    pub wReserved2: u16,
    pub wReserved3: u16,
    pub Anonymous: VARIANT_0_0_0,
}
impl Clone for VARIANT_0_0 {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for VARIANT_0_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
pub union VARIANT_0_0_0 {
    pub llVal: i64,
    pub lVal: i32,
    pub bVal: u8,
    pub iVal: i16,
    pub fltVal: f32,
    pub dblVal: f64,
    pub boolVal: VARIANT_BOOL,
    pub __OBSOLETE__VARIANT_BOOL: VARIANT_BOOL,
    pub scode: SCODE,
    pub cyVal: CY,
    pub date: f64,
    pub bstrVal: core::mem::ManuallyDrop<windows_core::BSTR>,
    pub punkVal: core::mem::ManuallyDrop<Option<windows_core::IUnknown>>,
    pub pdispVal: core::mem::ManuallyDrop<Option<IDispatch>>,
    pub parray: *mut SAFEARRAY,
    pub pbVal: *mut u8,
    pub piVal: *mut i16,
    pub plVal: *mut i32,
    pub pllVal: *mut i64,
    pub pfltVal: *mut f32,
    pub pdblVal: *mut f64,
    pub pboolVal: *mut VARIANT_BOOL,
    pub __OBSOLETE__VARIANT_PBOOL: *mut VARIANT_BOOL,
    pub pscode: *mut SCODE,
    pub pcyVal: *mut CY,
    pub pdate: *mut f64,
    pub pbstrVal: *mut windows_core::BSTR,
    pub ppunkVal: *mut Option<windows_core::IUnknown>,
    pub ppdispVal: *mut Option<IDispatch>,
    pub pparray: *mut *mut SAFEARRAY,
    pub pvarVal: *mut VARIANT,
    pub byref: *mut core::ffi::c_void,
    pub cVal: i8,
    pub uiVal: u16,
    pub ulVal: u32,
    pub ullVal: u64,
    pub intVal: i32,
    pub uintVal: u32,
    pub pdecVal: *mut DECIMAL,
    pub pcVal: *mut i8,
    pub puiVal: *mut u16,
    pub pulVal: *mut u32,
    pub pullVal: *mut u64,
    pub pintVal: *mut i32,
    pub puintVal: *mut u32,
    pub Anonymous: core::mem::ManuallyDrop<VARIANT_0_0_0_0>,
}
impl Clone for VARIANT_0_0_0 {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for VARIANT_0_0_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VARIANT_0_0_0_0 {
    pub pvRecord: *mut core::ffi::c_void,
    pub pRecInfo: core::mem::ManuallyDrop<Option<IRecordInfo>>,
}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct VARIANT_BOOL(pub i16);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct VARTYPE(pub u16);
#[repr(C)]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VERSIONEDSTREAM {
    pub guidVersion: windows_core::GUID,
    pub pStream: core::mem::ManuallyDrop<Option<IStream>>,
}
pub const VT_BSTR: VARENUM = 8;
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValueSet(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    ValueSet,
    windows_core::IUnknown,
    windows_core::IInspectable,
    IPropertySet
);
windows_core::imp::required_hierarchy!(ValueSet, windows_collections::IIterable < windows_collections::IKeyValuePair < windows_core::HSTRING, windows_core::IInspectable > >, windows_collections::IMap < windows_core::HSTRING, windows_core::IInspectable >, windows_collections::IObservableMap < windows_core::HSTRING, windows_core::IInspectable >);
impl ValueSet {
    pub fn new() -> windows_core::Result<Self> {
        Self::IActivationFactory(|f| f.ActivateInstance::<Self>())
    }
    fn IActivationFactory<
        R,
        F: FnOnce(&windows_core::imp::IGenericFactory) -> windows_core::Result<R>,
    >(
        callback: F,
    ) -> windows_core::Result<R> {
        static SHARED: windows_core::imp::FactoryCache<
            ValueSet,
            windows_core::imp::IGenericFactory,
        > = windows_core::imp::FactoryCache::new();
        SHARED.call(callback)
    }
    pub fn First(
        &self,
    ) -> windows_core::Result<
        windows_collections::IIterator<
            windows_collections::IKeyValuePair<windows_core::HSTRING, windows_core::IInspectable>,
        >,
    > {
        let this = &windows_core::Interface::cast::<
            windows_collections::IIterable<
                windows_collections::IKeyValuePair<
                    windows_core::HSTRING,
                    windows_core::IInspectable,
                >,
            >,
        >(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).First)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn Lookup(
        &self,
        key: &windows_core::HSTRING,
    ) -> windows_core::Result<windows_core::IInspectable> {
        let this = &windows_core::Interface::cast::<
            windows_collections::IMap<windows_core::HSTRING, windows_core::IInspectable>,
        >(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).Lookup)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(key),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn Size(&self) -> windows_core::Result<u32> {
        let this = &windows_core::Interface::cast::<
            windows_collections::IMap<windows_core::HSTRING, windows_core::IInspectable>,
        >(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).Size)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn HasKey(&self, key: &windows_core::HSTRING) -> windows_core::Result<bool> {
        let this = &windows_core::Interface::cast::<
            windows_collections::IMap<windows_core::HSTRING, windows_core::IInspectable>,
        >(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).HasKey)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(key),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn GetView(
        &self,
    ) -> windows_core::Result<
        windows_collections::IMapView<windows_core::HSTRING, windows_core::IInspectable>,
    > {
        let this = &windows_core::Interface::cast::<
            windows_collections::IMap<windows_core::HSTRING, windows_core::IInspectable>,
        >(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).GetView)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn Insert<P1>(&self, key: &windows_core::HSTRING, value: P1) -> windows_core::Result<bool>
    where
        P1: windows_core::Param<windows_core::IInspectable>,
    {
        let this = &windows_core::Interface::cast::<
            windows_collections::IMap<windows_core::HSTRING, windows_core::IInspectable>,
        >(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).Insert)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(key),
                value.param().abi(),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn Remove(&self, key: &windows_core::HSTRING) -> windows_core::Result<()> {
        let this = &windows_core::Interface::cast::<
            windows_collections::IMap<windows_core::HSTRING, windows_core::IInspectable>,
        >(self)?;
        unsafe {
            (windows_core::Interface::vtable(this).Remove)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(key),
            )
            .ok()
        }
    }
    pub fn Clear(&self) -> windows_core::Result<()> {
        let this = &windows_core::Interface::cast::<
            windows_collections::IMap<windows_core::HSTRING, windows_core::IInspectable>,
        >(self)?;
        unsafe {
            (windows_core::Interface::vtable(this).Clear)(windows_core::Interface::as_raw(this))
                .ok()
        }
    }
    pub fn MapChanged<F>(&self, handler: F) -> windows_core::Result<windows_core::EventRevoker>
    where
        F: Fn(
                windows_core::Ref<
                    windows_collections::IObservableMap<
                        windows_core::HSTRING,
                        windows_core::IInspectable,
                    >,
                >,
                windows_core::Ref<windows_collections::IMapChangedEventArgs<windows_core::HSTRING>>,
            ) + Send
            + 'static,
    {
        let this = &windows_core::Interface::cast::<
            windows_collections::IObservableMap<windows_core::HSTRING, windows_core::IInspectable>,
        >(self)?;
        let handler = <windows_collections::MapChangedEventHandler<
            windows_core::HSTRING,
            windows_core::IInspectable,
        >>::new(move |a0, a1| {
            handler(a0, a1);
            Ok(())
        });
        unsafe {
            let mut result__ = core::mem::zeroed();
            let token__ = (windows_core::Interface::vtable(this).MapChanged)(
                windows_core::Interface::as_raw(this),
                windows_core::Interface::as_raw(&handler),
                &mut result__,
            )
            .map(|| result__)?;
            Ok(windows_core::EventRevoker::new(
                this.clone(),
                token__,
                windows_core::Interface::vtable(this).RemoveMapChanged,
            ))
        }
    }
}
impl windows_core::RuntimeType for ValueSet {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_class::<Self, IPropertySet>();
}
unsafe impl windows_core::Interface for ValueSet {
    type Vtable = <IPropertySet as windows_core::Interface>::Vtable;
    const IID: windows_core::GUID = <IPropertySet as windows_core::Interface>::IID;
}
impl windows_core::RuntimeName for ValueSet {
    const NAME: &'static str = "Windows.Foundation.Collections.ValueSet";
}
unsafe impl Send for ValueSet {}
unsafe impl Sync for ValueSet {}
impl IntoIterator for ValueSet {
    type Item =
        windows_collections::IKeyValuePair<windows_core::HSTRING, windows_core::IInspectable>;
    type IntoIter = windows_collections::BufferedIterator<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        IntoIterator::into_iter(&self)
    }
}
impl IntoIterator for &ValueSet {
    type Item =
        windows_collections::IKeyValuePair<windows_core::HSTRING, windows_core::IInspectable>;
    type IntoIter = windows_collections::BufferedIterator<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        windows_collections::BufferedIterator::new(self.First().unwrap())
    }
}
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlDocument(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    XmlDocument,
    windows_core::IUnknown,
    windows_core::IInspectable
);
windows_core::imp::required_hierarchy!(XmlDocument, IXmlNode, IXmlNodeSelector, IXmlNodeSerializer);
impl XmlDocument {
    pub fn new() -> windows_core::Result<Self> {
        Self::IActivationFactory(|f| f.ActivateInstance::<Self>())
    }
    fn IActivationFactory<
        R,
        F: FnOnce(&windows_core::imp::IGenericFactory) -> windows_core::Result<R>,
    >(
        callback: F,
    ) -> windows_core::Result<R> {
        static SHARED: windows_core::imp::FactoryCache<
            XmlDocument,
            windows_core::imp::IGenericFactory,
        > = windows_core::imp::FactoryCache::new();
        SHARED.call(callback)
    }
    pub fn DocumentUri(&self) -> windows_core::Result<windows_core::HSTRING> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).DocumentUri)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn ImportNode<P0>(&self, node: P0, deep: bool) -> windows_core::Result<IXmlNode>
    where
        P0: windows_core::Param<IXmlNode>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).ImportNode)(
                windows_core::Interface::as_raw(self),
                node.param().abi(),
                deep,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn LoadXml(&self, xml: &windows_core::HSTRING) -> windows_core::Result<()> {
        let this = &windows_core::Interface::cast::<IXmlDocumentIO>(self)?;
        unsafe {
            (windows_core::Interface::vtable(this).LoadXml)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(xml),
            )
            .ok()
        }
    }
    pub fn NodeValue(&self) -> windows_core::Result<windows_core::IInspectable> {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).NodeValue)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn SetNodeValue<P0>(&self, value: P0) -> windows_core::Result<()>
    where
        P0: windows_core::Param<windows_core::IInspectable>,
    {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            (windows_core::Interface::vtable(this).SetNodeValue)(
                windows_core::Interface::as_raw(this),
                value.param().abi(),
            )
            .ok()
        }
    }
    pub fn NodeName(&self) -> windows_core::Result<windows_core::HSTRING> {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).NodeName)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn ParentNode(&self) -> windows_core::Result<IXmlNode> {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).ParentNode)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn FirstChild(&self) -> windows_core::Result<IXmlNode> {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).FirstChild)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn LastChild(&self) -> windows_core::Result<IXmlNode> {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).LastChild)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn PreviousSibling(&self) -> windows_core::Result<IXmlNode> {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).PreviousSibling)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn NextSibling(&self) -> windows_core::Result<IXmlNode> {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).NextSibling)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn HasChildNodes(&self) -> windows_core::Result<bool> {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).HasChildNodes)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn OwnerDocument(&self) -> windows_core::Result<Self> {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).OwnerDocument)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn InsertBefore<P0, P1>(
        &self,
        newchild: P0,
        referencechild: P1,
    ) -> windows_core::Result<IXmlNode>
    where
        P0: windows_core::Param<IXmlNode>,
        P1: windows_core::Param<IXmlNode>,
    {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).InsertBefore)(
                windows_core::Interface::as_raw(this),
                newchild.param().abi(),
                referencechild.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn ReplaceChild<P0, P1>(
        &self,
        newchild: P0,
        referencechild: P1,
    ) -> windows_core::Result<IXmlNode>
    where
        P0: windows_core::Param<IXmlNode>,
        P1: windows_core::Param<IXmlNode>,
    {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).ReplaceChild)(
                windows_core::Interface::as_raw(this),
                newchild.param().abi(),
                referencechild.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn RemoveChild<P0>(&self, childnode: P0) -> windows_core::Result<IXmlNode>
    where
        P0: windows_core::Param<IXmlNode>,
    {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).RemoveChild)(
                windows_core::Interface::as_raw(this),
                childnode.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn AppendChild<P0>(&self, newchild: P0) -> windows_core::Result<IXmlNode>
    where
        P0: windows_core::Param<IXmlNode>,
    {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).AppendChild)(
                windows_core::Interface::as_raw(this),
                newchild.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn CloneNode(&self, deep: bool) -> windows_core::Result<IXmlNode> {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).CloneNode)(
                windows_core::Interface::as_raw(this),
                deep,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn NamespaceUri(&self) -> windows_core::Result<windows_core::IInspectable> {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).NamespaceUri)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn LocalName(&self) -> windows_core::Result<windows_core::IInspectable> {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).LocalName)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn Prefix(&self) -> windows_core::Result<windows_core::IInspectable> {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).Prefix)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn Normalize(&self) -> windows_core::Result<()> {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            (windows_core::Interface::vtable(this).Normalize)(windows_core::Interface::as_raw(this))
                .ok()
        }
    }
    pub fn SetPrefix<P0>(&self, value: P0) -> windows_core::Result<()>
    where
        P0: windows_core::Param<windows_core::IInspectable>,
    {
        let this = &windows_core::Interface::cast::<IXmlNode>(self)?;
        unsafe {
            (windows_core::Interface::vtable(this).SetPrefix)(
                windows_core::Interface::as_raw(this),
                value.param().abi(),
            )
            .ok()
        }
    }
    pub fn SelectSingleNode(
        &self,
        xpath: &windows_core::HSTRING,
    ) -> windows_core::Result<IXmlNode> {
        let this = &windows_core::Interface::cast::<IXmlNodeSelector>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).SelectSingleNode)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(xpath),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn SelectSingleNodeNS<P1>(
        &self,
        xpath: &windows_core::HSTRING,
        namespaces: P1,
    ) -> windows_core::Result<IXmlNode>
    where
        P1: windows_core::Param<windows_core::IInspectable>,
    {
        let this = &windows_core::Interface::cast::<IXmlNodeSelector>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).SelectSingleNodeNS)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(xpath),
                namespaces.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub fn GetXml(&self) -> windows_core::Result<windows_core::HSTRING> {
        let this = &windows_core::Interface::cast::<IXmlNodeSerializer>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).GetXml)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn InnerText(&self) -> windows_core::Result<windows_core::HSTRING> {
        let this = &windows_core::Interface::cast::<IXmlNodeSerializer>(self)?;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).InnerText)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub fn SetInnerText(&self, value: &windows_core::HSTRING) -> windows_core::Result<()> {
        let this = &windows_core::Interface::cast::<IXmlNodeSerializer>(self)?;
        unsafe {
            (windows_core::Interface::vtable(this).SetInnerText)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(value),
            )
            .ok()
        }
    }
    fn IXmlDocumentStatics<R, F: FnOnce(&IXmlDocumentStatics) -> windows_core::Result<R>>(
        callback: F,
    ) -> windows_core::Result<R> {
        static SHARED: windows_core::imp::FactoryCache<XmlDocument, IXmlDocumentStatics> =
            windows_core::imp::FactoryCache::new();
        SHARED.call(callback)
    }
}
impl windows_core::RuntimeType for XmlDocument {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_class::<Self, IXmlDocument>();
}
unsafe impl windows_core::Interface for XmlDocument {
    type Vtable = <IXmlDocument as windows_core::Interface>::Vtable;
    const IID: windows_core::GUID = <IXmlDocument as windows_core::Interface>::IID;
}
impl windows_core::RuntimeName for XmlDocument {
    const NAME: &'static str = "Windows.Data.Xml.Dom.XmlDocument";
}
unsafe impl Send for XmlDocument {}
unsafe impl Sync for XmlDocument {}
