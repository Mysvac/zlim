use zlim_reflect::derive::impl_reflect;
use zlim_reflect::ops::Opaque;
use zlim_reflect::serde::ReflectContext;

use crate::asset::Asset;
use crate::handle::{ErasedHandle, Handle, HandleReference, TypedHandleReference};
use crate::ident::ErasedAssetId;
use crate::server::AssetServer;

// -----------------------------------------------------------------------------
// ReflectContext

#[expect(unsafe_code, reason = "AssetServer is unsafe")]
unsafe impl ReflectContext for AssetServer {}

// -----------------------------------------------------------------------------
// Handle & ErasedHandle

impl_reflect! {
    #[reflect(Opaque, Clone, Debug, Hash, Eq, Default)]
    #[reflect(Serialize, Deserialize)]
    pub struct Handle<A: Asset> {}
}

impl_reflect! {
    #[reflect(Opaque, Clone, Debug, Hash, Eq)]
    #[reflect(Serialize, Deserialize)]
    pub struct ErasedHandle {}
}

// -----------------------------------------------------------------------------
// Opaque

impl<A: Asset> Opaque for Handle<A> {
    fn apply_str(&mut self, _: &str) -> Result<(), String> {
        Err("Handle does not support Opaque::apply_str".into())
    }

    fn stringify(&self) -> String {
        self.as_reference().to_string()
    }
}

impl Opaque for ErasedHandle {
    fn apply_str(&mut self, _: &str) -> Result<(), String> {
        Err("ErasedHandle does not support Opaque::apply_str".into())
    }

    fn stringify(&self) -> String {
        self.as_reference().to_string()
    }
}

// -----------------------------------------------------------------------------
// serialize

impl<A: Asset> Handle<A> {
    fn as_reference(&self) -> HandleReference<'_> {
        match self {
            Handle::Uuid(uuid, ..) => HandleReference::Uuid(*uuid),
            Handle::Strong(handle) => match &handle.path {
                None => {
                    ::core::hint::cold_path();
                    zlim_log::warn!(
                        "Trying to serialize a StrongHandle `{handle:?}` without \
                        an AssetPath; falling back to the default Uuid."
                    );
                    HandleReference::Uuid(ErasedAssetId::DEFAULT_UUID)
                }
                Some(path) => HandleReference::Path(path.reborrow()),
            },
        }
    }
}

impl ErasedHandle {
    fn as_reference(&self) -> TypedHandleReference<'_> {
        match self {
            ErasedHandle::Uuid { uuid, type_id } => TypedHandleReference {
                type_id: *type_id,
                reference: HandleReference::Uuid(*uuid),
            },
            ErasedHandle::Strong(handle) => {
                let type_id = handle.type_id;
                if handle.path.is_none() {
                    ::core::hint::cold_path();
                    zlim_log::warn!(
                        "Trying to serialize a StrongHandle `{handle:?}` without \
                        an AssetPath; falling back to the default Uuid."
                    );
                }
                match &handle.path {
                    None => TypedHandleReference {
                        type_id,
                        reference: HandleReference::Uuid(ErasedAssetId::DEFAULT_UUID),
                    },
                    Some(path) => TypedHandleReference {
                        type_id,
                        reference: HandleReference::Path(path.reborrow()),
                    },
                }
            }
        }
    }
}

/// Handle Serialization
///
/// Format:
///
/// Uuid Handle: `urn:uuid:$uuid`.
/// For example: `urn:uuid:67e55044-10b1-426f-9247-bb680e5fe0c8`.
///
/// Strong Handle with Path: `$path`
/// For example: `http://example.png`.
///
/// Strong Handle without Path: `urn:uuid:$default_uuid`.
mod handle_serde {
    use core::marker::PhantomData;

    use super::*;
    use crate::handle::HandleReference;
    use erased_serde::Deserializer as ErasedDeserializer;
    use erased_serde::Error as ErasedError;
    use serde::Serialize;
    use zlim_reflect::Reflect;
    use zlim_reflect::serde::ReflectDeserialize;

    impl<A: Asset> Serialize for Handle<A> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            self.as_reference().serialize(serializer)
        }
    }

    impl<A: Asset> ReflectDeserialize for Handle<A> {
        fn reflect_deserialize(
            deserializer: &mut dyn ErasedDeserializer<'_>,
            ctx: &dyn ReflectContext,
        ) -> Result<Box<dyn Reflect>, ErasedError> {
            match erased_serde::deserialize::<HandleReference>(deserializer)? {
                HandleReference::Uuid(uuid) => Ok(Box::new(Handle::<A>::Uuid(uuid, PhantomData))),
                HandleReference::Path(path) => {
                    let Some(server) = ctx.get::<AssetServer>() else {
                        ::core::hint::cold_path();
                        let e = format!("Missing AssetServer in context `{}`", ctx.debug());
                        return Err(serde::de::Error::custom(e));
                    };
                    Ok(Box::new(server.load::<A>(path)))
                }
            }
        }
    }
}

/// Erased Handle Serialization
///
/// Format:
///
/// Uuid Handle: `[$type]|urn:uuid:$uuid`.
/// For example: `[Image]|urn:uuid:67e55044-10b1-426f-9247-bb680e5fe0c8`.
///
/// Strong Handle with Path: `[$type]|$path`
/// For example: `[Image]|http://example.png`.
///
/// Strong Handle without Path: `[$type]|urn:uuid:$default_uuid`.
mod erased_handle_serde {
    use super::*;
    use crate::handle::{HandleReference, TypedHandleReference};
    use erased_serde::Deserializer as ErasedDeserializer;
    use erased_serde::Error as ErasedError;
    use serde::Serialize;
    use zlim_reflect::Reflect;
    use zlim_reflect::serde::ReflectDeserialize;

    impl Serialize for ErasedHandle {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            self.as_reference().serialize(serializer)
        }
    }

    impl ReflectDeserialize for ErasedHandle {
        fn reflect_deserialize(
            deserializer: &mut dyn ErasedDeserializer<'_>,
            ctx: &dyn ReflectContext,
        ) -> Result<Box<dyn Reflect>, ErasedError> {
            let TypedHandleReference { type_id, reference } =
                erased_serde::deserialize(deserializer)?;

            match reference {
                HandleReference::Uuid(uuid) => Ok(Box::new(ErasedHandle::Uuid { uuid, type_id })),
                HandleReference::Path(path) => {
                    let Some(server) = ctx.get::<AssetServer>() else {
                        ::core::hint::cold_path();
                        let e = format!("Missing AssetServer in context `{}`", ctx.debug());
                        return Err(serde::de::Error::custom(e));
                    };
                    let handle = server.load_builder().load_erased(type_id, path);
                    Ok(Box::new(handle))
                }
            }
        }
    }
}

// -----------------------------------------------------------------------------
