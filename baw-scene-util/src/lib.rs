use bevy_scene::{ResolveContext, ResolveSceneError, ResolvedScene, Scene};

/// Type used for identifying unique scene types in bevy scenes. This is used to uniquely identify a
/// scene type so that SceneEntityReferences don't collide.
pub type SceneTypeId = (&'static str, usize, usize);

/// Generates a unique type id tuple for a SceneEntityReference.
///
/// The generated tuple consists of the current file path, line, and column, and uniquely identifies
/// a particular scene type so that entity references from that scene type don't collide with ones
/// from other scenes.
///
/// For a hand-written scene struct, you would generally create this once and store it in a
/// constant to use across entity references created by that scene.
///
/// ```
/// use baw_scene_util::{scene_type_id, SceneTypeId};
/// use bevy_ecs::template::SceneEntityReference;
/// use bevy_scene::macro_utils::CallCounter;
/// use bevy_scene::{
///     NameEntityReference,
///     ResolveContext,
///     ResolvedScene,
///     ResolveSceneError,
///     Scene,
/// };
///
/// struct MyScene {
///     /// ID which unitquely identifies this Scene instance from other Scenes of the same type as
///     /// MyScene.
///     instance_id: u64,
/// }
///
/// impl MyScene {
///     const MY_SCENE_TYPE_ID: SceneTypeId = scene_type_id!();
///
///     fn new() -> Self {
///         static INSTANCE_ID: CallCounter = CallCounter::new();
///         Self {
///             instance_id: INSTANCE_ID.increment(),
///         }
///     }
/// }
///
/// impl Scene for MyScene {
///     fn resolve(
///         self,
///         context: &mut ResolveContext,
///         scene: &mut ResolvedScene,
///     ) -> Result<(), ResolveSceneError> {
///         // Number which uniquely identifies the entity of a SceneEntityReference amongst
///         // entities generated with the same scene type id and scene instance id.
///         let entity_id = 0;
///         NameEntityReference {
///             name: "my_scene".into(),
///             reference: SceneEntityReference::new(Self::MY_SCENE_TYPE_ID, entity_id, self.instance_id),
///         }.resolve_inline(context, scene);
///         Ok(())
///     }
/// }
/// ```
#[doc(hidden)]
#[macro_export]
macro_rules! __scene_type_id {
    () => {
        (file!(), line!() as usize, column!() as usize)
    };
}

/// Re-exports of bevy crates for macros.
#[doc(hidden)]
pub mod bevy {
    pub use bevy_ecs as ecs;
}

/// Helper for assigning template values within handwritten resolve functions.
///
/// Takes a context, scene, and component type and applies `get_or_insert_template` for the
/// component type's template. Allows binding a temporary identifier to modify the template.
///
/// ```
/// # use bevy_scene::{
/// #     ResolveContext,
/// #     ResolvedScene,
/// #     ResolveSceneError,
/// #     Scene,
/// # };
/// # use bevy_ecs::component::Component;
/// #
/// # #[derive(Default, Clone, Component)]
/// # struct SomeComponent {
/// #     some_field: i32,
/// # }
///
/// use baw_scene_util::with_template;
///
/// # struct MyScene;
/// # impl Scene for MyScene {
/// fn resolve(
///     self,
///     context: &mut ResolveContext,
///     scene: &mut ResolvedScene,
/// ) -> Result<(), ResolveSceneError> {
///     // Ensures a SomeComponent template is added, but does not change any fields from the
///     // defaults or whatever the template was previously set to.
///     with_template!(context, scene, SomeComponent);
///     // Ensures a SomeComponent template is added, and overrides the some_field.
///     with_template!(context, scene, SomeComponent, |some_component| {
///         some_component.some_field = 3;
///     });
///     Ok(())
/// }
/// # }
/// ```
#[doc(hidden)]
#[macro_export]
macro_rules! __with_template {
    ($context:ident, $scene:ident,  $type:ty) => {
        $crate::with_template!($context, $scene, $type, |_template| {})
    };
    ($context:ident, $scene:ident, $type:ty, |$name:ident| $expr:expr) => {{
        let $name = $scene.get_or_insert_template::<<$type as $crate::bevy::ecs::template::FromTemplate>::Template>($context);
        $expr
    }};
}

pub use __scene_type_id as scene_type_id;
pub use __with_template as with_template;
use bevy_transform::components::Transform;

/// Utility scene which scales the current scene's [`Transform`] by the given factor.
pub struct ScaleScene(pub f32);

impl Scene for ScaleScene {
    fn resolve(
        self,
        context: &mut ResolveContext,
        scene: &mut ResolvedScene,
    ) -> Result<(), ResolveSceneError> {
        with_template!(context, scene, Transform, |trans| {
            trans.scale *= self.0;
        });
        Ok(())
    }
}
