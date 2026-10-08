use zlim_error::ZlimResult;

use super::{Template, TemplateContext};

// -----------------------------------------------------------------------------
// TemplateTuple

/// A tuple of [`Template`]s, which is itself a [`Template`].
#[repr(transparent)]
pub struct TemplateTuple<T>(pub T);

/// Generates [`Template`] implementations for tuples.
macro_rules! impl_template_for_tuple {
    (0: []) => {
        impl Template for TemplateTuple<()> {
            type Output = ();

            #[inline(always)]
            fn build_template(&self, _: &mut TemplateContext) -> ZlimResult<Self::Output> {
                Ok(())
            }

            #[inline(always)]
            fn clone_template(&self) -> Self {
                TemplateTuple(())
            }
        }
    };
    (1 : [ $index:tt : $template:ident ]) => {
        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 templates long.")]
        impl<$template: Template> Template for TemplateTuple<($template,)> {
            type Output = ($template::Output,);

            #[inline]
            #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
            fn build_template(&self, context: &mut TemplateContext) -> ZlimResult<Self::Output> {
                Ok((self.0.$index.build_template(context)?,))
            }

            #[inline]
            fn clone_template(&self) -> Self {
                TemplateTuple((self.0.$index.clone_template(),))
            }
        }
    };
    ($num:literal : [$($index:tt : $template:ident),*]) => {
        #[cfg_attr(docsrs, doc(hidden))]
        impl<$($template: Template),*> Template for TemplateTuple<($($template,)*)> {
            type Output = ($($template::Output,)*);

            #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
            fn build_template(&self, context: &mut TemplateContext) -> ZlimResult<Self::Output> {
                Ok(($(self.0.$index.build_template(context)?,)*))
            }

            fn clone_template(&self) -> Self {
                TemplateTuple(($(self.0.$index.clone_template(),)*))
            }
        }
    };
}

zlim_utils::range_invoke!(impl_template_for_tuple, 12);
